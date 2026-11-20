//! Ponte com a extensão do navegador: servidor do pipe, pareamento e avisos de estado.
//! Bridge with the browser extension: pipe server, pairing and status notices.
//!
//! Camadas de proteção (do planejamento) / Protection layers (from the plan):
//! 1. Pipe só do usuário atual e que não pode ser sequestrado (`sigilvault_ipc::pipe`).
//!    Pipe for the current user only that cannot be hijacked.
//! 2. Do outro lado precisa estar o `sigilvault-host.exe` da mesma pasta, com a mesma assinatura.
//!    The other side must be `sigilvault-host.exe` from the same folder, with the same signature.
//! 3. Só as nossas extensões (ID informado pelo navegador). / Only our extensions (ID given by the browser).
//! 4. Pareamento OPAQUE com código mostrado no app e confirmação do usuário; depois, canal
//!    criptografado por conexão. / OPAQUE pairing with a code shown in the app and user
//!    confirmation; then an encrypted channel per connection.

pub mod commands;
mod pairings;
mod session;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sigilvault_ipc::pipe::{self, Listener};
use sigilvault_pairing::{Channel, FillRng, PairingCode, Role, ServerLogin, ServerPairing, pairing_key};
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

pub use pairings::Pairings;
use pairings::{Pairing, now_secs};
use session::Conn;

use crate::state::AppState;
use crate::storage::Paths;
use crate::window;

/// Validade do código de pareamento. / Pairing code lifetime.
const PAIRING_TTL: Duration = Duration::from_secs(120);
/// Tentativas por código. / Attempts per code.
const PAIRING_ATTEMPTS: u8 = 3;
/// Intervalo mínimo entre pedidos de pareamento. / Minimum interval between pairing requests.
const PAIR_START_COOLDOWN: Duration = Duration::from_secs(3);
/// Conexões simultâneas (uma por navegador aberto, normalmente). / Simultaneous connections (usually one per open browser).
const MAX_CONNECTIONS: usize = 16;

/// Acaso do sistema para o pareamento. / System randomness for pairing.
fn fill(buf: &mut [u8]) {
    getrandom::fill(buf).expect("gerador do sistema indisponível / system RNG unavailable");
}

fn rng() -> FillRng<impl FnMut(&mut [u8])> {
    FillRng(fill)
}

/// Pareamento em andamento. / Pairing in progress.
struct Pending {
    code: PairingCode,
    browser: String,
    caller: String,
    /// Só a conexão que pediu pode continuar. / Only the requesting connection may continue.
    owner: u64,
    server: ServerPairing,
    login: Option<ServerLogin>,
    attempts_left: u8,
    expires: Instant,
}

struct BridgeInner {
    pairings: Pairings,
    pending: Option<Pending>,
    conns: Vec<Weak<Conn>>,
    last_pair_start: Option<Instant>,
}

/// Estado da ponte (gerenciado pelo Tauri). / Bridge state (managed by Tauri).
pub struct Bridge {
    inner: Mutex<BridgeInner>,
    next_conn: AtomicU64,
    active: AtomicUsize,
}

impl Bridge {
    pub fn new(paths: &Paths) -> Self {
        Self {
            inner: Mutex::new(BridgeInner { pairings: Pairings::load(&paths.pairings), pending: None, conns: Vec::new(), last_pair_start: None }),
            next_conn: AtomicU64::new(1),
            active: AtomicUsize::new(0),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BridgeInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl BridgeInner {
    /// Descarta o pareamento pendente vencido. Devolve se descartou. / Drops an expired pending pairing.
    fn expire_pending(&mut self) -> bool {
        if self.pending.as_ref().is_some_and(|p| Instant::now() >= p.expires) {
            self.pending = None;
            return true;
        }
        false
    }

    fn live_conns(&mut self) -> Vec<Arc<Conn>> {
        self.conns.retain(|w| w.strong_count() > 0);
        self.conns.iter().filter_map(Weak::upgrade).collect()
    }
}

/// Inicia o servidor do pipe. Se outro programa já ocupa o nome, a ponte fica desligada (a
/// extensão verá o app como indisponível) em vez de dividir o nome com ele.
/// Starts the pipe server. If another program already holds the name, the bridge stays off (the
/// extension will see the app as unavailable) instead of sharing the name with it.
pub fn start(app: &AppHandle) {
    let listener = pipe::default_name().and_then(|name| Listener::bind(&name));
    let mut listener = match listener {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ponte desligada: pipe indisponível / bridge off: pipe unavailable: {e}");
            return;
        }
    };
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            match listener.accept() {
                Ok(conn) => {
                    let bridge = app.state::<Bridge>();
                    if bridge.active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                        continue; // soltar = desconectar / dropping = disconnecting
                    }
                    bridge.active.fetch_add(1, Ordering::SeqCst);
                    let id = bridge.next_conn.fetch_add(1, Ordering::SeqCst);
                    let app = app.clone();
                    std::thread::spawn(move || {
                        session::serve(&app, conn, id);
                        let bridge = app.state::<Bridge>();
                        bridge.active.fetch_sub(1, Ordering::SeqCst);
                        connection_closed(&app, id);
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    });
}

/// Estado do cofre para a extensão. / Vault status for the extension.
fn status(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let inner = state.lock();
    match &inner.vault {
        Some(v) => json!({ "locked": false, "username": v.username() }),
        None => json!({ "locked": true }),
    }
}

/// Avisa todas as extensões conectadas que o cofre trancou ou abriu. Chamar sem o estado travado.
/// Tells all connected extensions the vault locked or unlocked. Call without the state locked.
pub fn notify_status(app: &AppHandle) {
    let Some(bridge) = app.try_state::<Bridge>() else { return };
    let mut msg = status(app);
    msg["ev"] = json!("status");
    let conns = bridge.lock().live_conns();
    for conn in conns {
        conn.send_box(&msg);
    }
    // O código só faz sentido com o cofre aberto. / The code only makes sense with the vault unlocked.
    if msg["locked"] == json!(true) {
        cancel_pending(app);
    }
}

/// Avisa a janela que o pareamento mudou. / Tells the window the pairing changed.
fn emit_pairing(app: &AppHandle, event: &str, browser: Option<&str>) {
    let _ = app.emit("pairing", json!({ "event": event, "browser": browser }));
}

/// Cancela o pareamento pendente (se houver). / Cancels the pending pairing (if any).
pub fn cancel_pending(app: &AppHandle) {
    let Some(bridge) = app.try_state::<Bridge>() else { return };
    if bridge.lock().pending.take().is_some() {
        emit_pairing(app, "cancelled", None);
    }
}

fn connection_closed(app: &AppHandle, id: u64) {
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    b.conns.retain(|w| w.upgrade().is_some_and(|c| c.id != id));
    if b.pending.as_ref().is_some_and(|p| p.owner == id) {
        b.pending = None;
        drop(b);
        emit_pairing(app, "cancelled", None);
    }
}

// ── Pareamento / Pairing ──────────────────────────────────────────

/// Nome do navegador vindo da extensão: só para mostrar, então é limpo e curto.
/// Browser name from the extension: display only, so it is cleaned and short.
fn clean_browser_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(24).collect();
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() { "Navegador".into() } else { cleaned }
}

/// Passo 0: a extensão pede para parear. O app mostra o código na janela.
/// Step 0: the extension asks to pair. The app shows the code in the window.
fn start_pairing(app: &AppHandle, conn: &Conn, browser: &str) -> Result<u64, &'static str> {
    let unlocked = app.state::<AppState>().lock().vault.is_some();
    if !unlocked {
        // Mostra a tela de desbloqueio; a extensão pede para tentar de novo depois.
        // Shows the unlock screen; the extension asks to try again afterwards.
        window::open(app);
        return Err("locked");
    }
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    b.expire_pending();
    if b.pending.as_ref().is_some_and(|p| p.owner != conn.id) {
        return Err("busy");
    }
    if b.last_pair_start.is_some_and(|t| t.elapsed() < PAIR_START_COOLDOWN) {
        return Err("slow_down");
    }
    if b.pairings.is_full() {
        return Err("too_many_pairings");
    }
    let code = PairingCode::generate(&mut fill);
    let server = ServerPairing::new(&mut rng(), &code).map_err(|_| "internal")?;
    let browser = clean_browser_name(browser);
    b.pending = Some(Pending {
        code,
        browser: browser.clone(),
        caller: conn.caller.clone(),
        owner: conn.id,
        server,
        login: None,
        attempts_left: PAIRING_ATTEMPTS,
        expires: Instant::now() + PAIRING_TTL,
    });
    b.last_pair_start = Some(Instant::now());
    drop(b);
    window::open(app);
    emit_pairing(app, "pending", Some(&browser));
    Ok(PAIRING_TTL.as_secs())
}

/// Passo 1 da OPAQUE. Cada chamada gasta uma tentativa. / OPAQUE step 1. Each call uses one attempt.
fn pair_login1(app: &AppHandle, conn: &Conn, request: &[u8]) -> Result<Vec<u8>, &'static str> {
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    if b.expire_pending() {
        drop(b);
        emit_pairing(app, "cancelled", None);
        return Err("expired");
    }
    let Some(p) = b.pending.as_mut().filter(|p| p.owner == conn.id) else { return Err("no_pairing") };
    if p.attempts_left == 0 {
        b.pending = None;
        drop(b);
        emit_pairing(app, "cancelled", None);
        return Err("too_many_attempts");
    }
    p.attempts_left -= 1;
    let (login, response) = p.server.login_start(&mut rng(), request).map_err(|_| "bad_message")?;
    p.login = Some(login);
    Ok(response)
}

/// Passo 2 da OPAQUE: se a prova da extensão confere, cria e grava o pareamento.
/// OPAQUE step 2: if the extension's proof checks out, creates and saves the pairing.
fn pair_login2(app: &AppHandle, conn: &Conn, finalization: &[u8]) -> Result<[u8; 16], &'static str> {
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    let Some(p) = b.pending.as_mut().filter(|p| p.owner == conn.id) else { return Err("no_pairing") };
    let Some(login) = p.login.take() else { return Err("out_of_order") };
    let session = match login.finish(finalization) {
        Ok(s) => s,
        Err(_) => {
            if p.attempts_left == 0 {
                b.pending = None;
                drop(b);
                emit_pairing(app, "cancelled", None);
            }
            return Err("bad_code");
        }
    };
    let Some(pending) = b.pending.take() else { return Err("no_pairing") };
    let mut id = [0u8; 16];
    fill(&mut id);
    let now = now_secs();
    let pairing = Pairing {
        id,
        caller: pending.caller.clone(),
        browser: pending.browser.clone(),
        created_at: now,
        last_seen: now,
        key: Zeroizing::new(pairing_key(&session, &id)),
    };
    let saved = b.pairings.add(pairing);
    drop(b);
    if saved.is_err() {
        emit_pairing(app, "cancelled", None);
        return Err("io");
    }
    emit_pairing(app, "paired", Some(&pending.browser));
    Ok(id)
}

/// Abre o canal de uma extensão já pareada. / Opens the channel of an already paired extension.
fn authenticate(app: &AppHandle, conn: &Arc<Conn>, id: &[u8; 16], extension_nonce: &[u8; 32]) -> Result<(), &'static str> {
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    let Some(pairing) = b.pairings.find(id, &conn.caller) else { return Err("unknown_pairing") };
    let mut app_nonce = [0u8; 32];
    fill(&mut app_nonce);
    let channel = Channel::new(&pairing.key, extension_nonce, &app_nonce, Role::App);
    if !conn.authenticate(channel, *id, &app_nonce) {
        return Err("already_authenticated");
    }
    b.pairings.touch(id);
    b.conns.push(Arc::downgrade(conn));
    Ok(())
}

/// Remove um pareamento e derruba as conexões dele. / Removes a pairing and drops its connections.
fn remove_pairing(app: &AppHandle, id: &[u8; 16]) -> std::io::Result<bool> {
    let bridge = app.state::<Bridge>();
    let mut b = bridge.lock();
    let removed = b.pairings.remove(id)?;
    let conns = b.live_conns();
    drop(b);
    for conn in conns.iter().filter(|c| c.pairing_id() == Some(*id)) {
        // Aviso autenticado: a extensão apaga a chave dela. / Authenticated notice: the extension deletes its key.
        conn.send_box(&json!({ "ev": "unpaired" }));
        conn.close();
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::clean_browser_name;

    #[test]
    fn browser_names_are_cleaned() {
        assert_eq!(clean_browser_name("Brave"), "Brave");
        assert_eq!(clean_browser_name("<img src=x onerror=alert(1)>"), "img srcx onerroralert1");
        assert_eq!(clean_browser_name(""), "Navegador");
        assert_eq!(clean_browser_name(&"a".repeat(100)).len(), 24);
    }
}
