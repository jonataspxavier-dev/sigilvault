//! Campos de senha de outros programas. No Windows o SigilVault desenha um selo em cima do
//! campo; no Linux (Wayland não permite isso) ele mostra uma notificação com o botão "Abrir o
//! SigilVault aqui". Os dois caminhos geram os mesmos eventos para o resto do app.
//! Other programs' password fields. On Windows SigilVault draws a seal over the field; on Linux
//! (Wayland does not allow that) it shows a notification with the "Open SigilVault here" button.
//! Both paths produce the same events for the rest of the app.
//!
//! O aviso "Guardar este login?" (`Event::LoginDone`) vem de vigiar o campo: quando ele some depois
//! de usado, a pessoa provavelmente entrou. No Windows isso só começa depois que o foco sai do campo;
//! no Linux, como a acessibilidade não avisa de forma confiável quando o foco sai, vigia-se desde o início.
//! The "Keep this login?" notice (`Event::LoginDone`) comes from watching the field: when it goes away
//! after being used, the person probably signed in. On Windows this only starts after focus leaves the
//! field; on Linux, since accessibility does not reliably report focus leaving, it watches from the start.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::atspi::{self, Element, Focus, ROLE_PASSWORD_TEXT};
use super::target::Target;

/// O que fazer num programa. / What to do in a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Policy {
    /// Mostrar o selo. / Show the seal.
    pub seal: bool,
    /// Oferecer salvar depois do login. / Offer to save after the login.
    pub offer: bool,
}

/// Campo de senha encontrado (sem a senha, claro). / Password field found (without the password, of course).
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// Identifica este campo nas chamadas seguintes. / Identifies this field in later calls.
    pub token: u64,
    /// Janela principal do programa (raiz). / Program's main (root) window.
    pub target: Target,
    /// Texto do campo de usuário ao lado, se houver. / Text of the nearby username field, if any.
    pub username: String,
    /// Achamos um campo de usuário? / Did we find a username field?
    pub has_username_field: bool,
}

/// Avisos para o app. / Notices to the app.
pub enum Event {
    /// Clicou no selo. / Clicked the seal.
    SealClicked(FieldInfo),
    /// Pediu "não mostrar neste programa". / Asked "do not show in this program".
    Ignore(PathBuf),
    /// O campo de senha sumiu depois de usado: provavelmente entrou. / The password field went away after use: probably signed in.
    LoginDone(FieldInfo),
}

/// Qual campo focar antes de digitar. / Which field to focus before typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Username,
    Password,
}

/// Ícone do selo em RGBA (de cima para baixo). Sem uso no Linux (a notificação usa o ícone do tema).
/// Seal icon in RGBA (top to bottom). Unused on Linux (the notification uses the theme icon).
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Campos vistos: o elemento de cada um, para preencher depois. / Fields seen: each one's element, to fill later.
static FIELDS: Mutex<Option<HashMap<u64, Element>>> = Mutex::new(None);
/// Textos da notificação (abrir, ignorar). / Notification texts (open, ignore).
static LABELS: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
/// Última notificação por programa, para não repetir a cada clique no mesmo campo. / Last notification per program, so it does not repeat on every click in the same field.
static LAST_SHOWN: Mutex<Option<HashMap<u32, Instant>>> = Mutex::new(None);
/// Campos preenchidos por nós (não se oferece guardar o login que o SigilVault mesmo preencheu).
/// Fields filled by us (no offer to keep a login that SigilVault itself filled).
static FILLED: Mutex<Option<HashSet<u64>>> = Mutex::new(None);
/// Campos já vigiados (nome no barramento + caminho), para não vigiar o mesmo duas vezes.
/// Fields already watched (bus name + path), so the same one is not watched twice.
static WATCHED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// De quanto em quanto tempo olhar se o campo ainda existe. / How often to check whether the field still exists.
const WATCH_EVERY: Duration = Duration::from_millis(1500);
/// Quantas olhadas no máximo (5 minutos). / Maximum number of checks (5 minutes).
const WATCH_TICKS: u32 = 200;
/// Um campo que some antes disso não conta (janela que piscou). / A field that goes away before this does not count (a window that flickered).
const MIN_LIFETIME: Duration = Duration::from_secs(2);
/// Intervalo mínimo entre avisos do mesmo programa. / Minimum interval between notices of the same program.
const QUIET: Duration = Duration::from_secs(20);
/// Quantos campos lembrar. / How many fields to remember.
const MAX_FIELDS: usize = 8;

fn lock<T>(m: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Começa a vigiar os campos de senha. / Starts watching password fields.
pub fn start(_icon: Icon, policy: impl Fn(&Path) -> Policy + Send + 'static, on_event: impl Fn(Event) + Send + 'static) {
    let policy = Mutex::new(policy);
    let on_event = std::sync::Arc::new(Mutex::new(on_event));
    atspi::on_focus(move |focus: &Focus| {
        if focus.role != ROLE_PASSWORD_TEXT || is_browser(&focus.exe) {
            return;
        }
        let allowed = (policy.lock().unwrap_or_else(|e| e.into_inner()))(&focus.exe);
        if allowed.offer {
            watch(focus.clone(), on_event.clone());
        }
        if !allowed.seal {
            return;
        }
        {
            let mut shown = lock(&LAST_SHOWN);
            let map = shown.get_or_insert_with(HashMap::new);
            if map.get(&focus.pid).is_some_and(|t| t.elapsed() < QUIET) {
                return;
            }
            map.insert(focus.pid, Instant::now());
        }
        {
            let mut fields = lock(&FIELDS);
            let map = fields.get_or_insert_with(HashMap::new);
            if map.len() >= MAX_FIELDS {
                map.clear();
            }
            map.insert(focus.token, focus.element.clone());
        }
        announce(focus.clone(), on_event.clone());
    });
    atspi::start();
}

/// Vigia um campo de senha: se ele some depois de ter sido usado (a janela de login fechou), a pessoa
/// provavelmente entrou, e o app oferece guardar o login. Nunca lemos o que foi digitado.
/// Watches a password field: if it goes away after being used (the login window closed), the person
/// probably signed in, and the app offers to keep the login. We never read what was typed.
fn watch(focus: Focus, on_event: std::sync::Arc<Mutex<impl Fn(Event) + Send + 'static>>) {
    let key = format!("{}{}", focus.element.bus, focus.element.path.as_str());
    if !lock(&WATCHED).get_or_insert_with(HashSet::new).insert(key.clone()) {
        return;
    }
    let Some(rt) = atspi::runtime_handle() else { return };
    rt.spawn(async move {
        let started = Instant::now();
        for _ in 0..WATCH_TICKS {
            tokio::time::sleep(WATCH_EVERY).await;
            if lock(&FILLED).as_ref().is_some_and(|f| f.contains(&focus.token)) {
                break;
            }
            if atspi::showing(&focus.element).await == Some(false) {
                if started.elapsed() >= MIN_LIFETIME {
                    let info = FieldInfo {
                        token: focus.token,
                        target: Target { hwnd: usize::try_from(focus.token).unwrap_or(0), pid: focus.pid, exe: focus.exe.clone(), title: focus.app_name.clone() },
                        username: String::new(),
                        has_username_field: false,
                    };
                    (on_event.lock().unwrap_or_else(|e| e.into_inner()))(Event::LoginDone(info));
                }
                break;
            }
        }
        lock(&WATCHED).get_or_insert_with(HashSet::new).remove(&key);
    });
}

fn is_browser(exe: &Path) -> bool {
    Target { hwnd: 0, pid: 0, exe: exe.to_path_buf(), title: String::new() }.is_browser()
}

/// Mostra a notificação do selo para este campo. / Shows the seal notification for this field.
fn announce(focus: Focus, on_event: std::sync::Arc<Mutex<impl Fn(Event) + Send + 'static>>) {
    let (open, ignore) = lock(&LABELS).clone();
    let english = open.starts_with("Open");
    let summary = "SigilVault";
    let body = if english { format!("Password field in {}", focus.app_name) } else { format!("Campo de senha em {}", focus.app_name) };
    let (open, ignore) = if open.is_empty() { ("Abrir o SigilVault aqui".to_owned(), "Não mostrar neste programa".to_owned()) } else { (open, ignore) };
    let info = FieldInfo {
        token: focus.token,
        target: Target { hwnd: usize::try_from(focus.token).unwrap_or(0), pid: focus.pid, exe: focus.exe.clone(), title: focus.app_name.clone() },
        username: String::new(),
        has_username_field: false,
    };
    let exe = focus.exe;
    let Some(rt) = super::atspi::runtime_handle() else { return };
    rt.spawn(async move {
        let actions = [("open", open.as_str()), ("ignore", ignore.as_str())];
        super::notify::show(summary, &body, &actions, move |key| {
            let event = match key {
                "open" => Event::SealClicked(info.clone()),
                "ignore" => Event::Ignore(exe.clone()),
                _ => return,
            };
            (on_event.lock().unwrap_or_else(|e| e.into_inner()))(event);
        })
        .await;
    });
}

/// Põe o foco num campo do programa. Só o campo de senha é conhecido no Linux.
/// Focuses a field of the program. Only the password field is known on Linux.
pub fn focus(token: u64, slot: Slot) -> bool {
    if slot != Slot::Password {
        return false;
    }
    let Some(el) = lock(&FIELDS).as_ref().and_then(|m| m.get(&token)).cloned() else { return false };
    // Mesmo que o programa não deixe mover o foco, o texto pode ser escrito no campo.
    // Even if the program does not allow moving focus, the text can still be written to the field.
    let _ = atspi::grab_focus(&el);
    super::typer::set_current(Some(el));
    true
}

/// Marca o campo como preenchido por nós. / Marks the field as filled by us.
pub fn mark_filled(token: u64) {
    lock(&FILLED).get_or_insert_with(HashSet::new).insert(token);
}

/// Esconde o selo. Notificações somem sozinhas. / Hides the seal. Notifications go away on their own.
pub fn hide_seal() {}

/// Textos do botão da notificação. / Notification button texts.
pub fn set_menu_labels(open: &str, ignore: &str) {
    *lock(&LABELS) = (open.to_owned(), ignore.to_owned());
}

/// Anotação de depuração. Sem efeito no Linux por enquanto. / Debug note. No effect on Linux for now.
pub fn trace(_msg: &str) {}

#[cfg(test)]
mod tests {
    use super::super::typer::{self, Part};
    use super::*;

    /// Usa o desktop de verdade (abre um diálogo de senha por alguns segundos): rode com
    /// `cargo test -- --ignored`. Simula o clique no botão da notificação.
    /// Uses the real desktop (opens a password dialog for a few seconds): run with
    /// `cargo test -- --ignored`. Simulates the click on the notification button.
    #[test]
    #[ignore = "abre uma janela e usa o D-Bus / opens a window and uses D-Bus"]
    fn password_field_triggers_the_notification_and_can_be_filled() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        start(
            Icon { rgba: Vec::new(), width: 0, height: 0 },
            |_| Policy { seal: true, offer: false },
            move |event| {
                if let Event::SealClicked(info) = event {
                    let _ = tx.lock().unwrap().send(info);
                }
            },
        );
        std::thread::sleep(Duration::from_millis(1500));
        let mut dialog = std::process::Command::new("zenity").args(["--password", "--title=sigilvault-test"]).spawn().expect("zenity");
        let started = Instant::now();
        while super::super::notify::last_id() == 0 && started.elapsed() < Duration::from_secs(8) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let id = super::super::notify::last_id();
        let emitted = std::process::Command::new("gdbus")
            .args(["emit", "--session", "--object-path", "/org/freedesktop/Notifications", "--signal", "org.freedesktop.Notifications.ActionInvoked", &format!("uint32 {id}"), "open"])
            .status()
            .unwrap();
        let clicked = rx.recv_timeout(Duration::from_secs(5));
        let outcome = clicked.as_ref().map(|info| {
            assert!(focus(info.token, Slot::Password));
            typer::type_into(&info.target, &[Part::Text(zeroize::Zeroizing::new("segredo-de-teste".into()))])
        });
        let _ = dialog.kill();
        assert!(id != 0, "nenhuma notificação foi mostrada / no notification was shown");
        assert!(emitted.success());
        assert_eq!(outcome.expect("sem clique / no click"), Ok(()));
    }

    /// Roda o diálogo de senha e o fecha; devolve se chegou `LoginDone` em até 10 s. `filled` marca o
    /// campo como preenchido por nós antes de fechar. / Runs the password dialog and closes it; returns
    /// whether `LoginDone` arrived within 10 s. `filled` marks the field as filled by us before closing.
    fn login_done_after_closing(filled: bool) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        start(
            Icon { rgba: Vec::new(), width: 0, height: 0 },
            |_| Policy { seal: false, offer: true },
            move |event| {
                if let Event::LoginDone(info) = event {
                    let _ = tx.lock().unwrap().send(info.token);
                }
            },
        );
        std::thread::sleep(Duration::from_millis(1500));
        let mut dialog = std::process::Command::new("zenity").args(["--password", "--title=sigilvault-test-login"]).spawn().expect("zenity");
        let last = Instant::now();
        // Espera o foco no campo (o vigia nasce com ele). / Waits for focus on the field (the watcher is born with it).
        let token = loop {
            if let Some(f) = atspi::last_focus().filter(|f| f.role == ROLE_PASSWORD_TEXT && f.exe.ends_with("zenity")) {
                break f.token;
            }
            assert!(last.elapsed() < Duration::from_secs(8), "o campo não ganhou foco / the field did not get focus");
            std::thread::sleep(Duration::from_millis(100));
        };
        if filled {
            mark_filled(token);
        }
        std::thread::sleep(Duration::from_secs(3));
        let _ = dialog.kill();
        let _ = dialog.wait();
        rx.recv_timeout(Duration::from_secs(10)).is_ok_and(|t| t == token)
    }

    /// Usa o desktop de verdade: rode com `cargo test -- --ignored --test-threads=1`.
    /// Uses the real desktop: run with `cargo test -- --ignored --test-threads=1`.
    #[test]
    #[ignore = "abre uma janela e usa o D-Bus / opens a window and uses D-Bus"]
    fn closing_the_login_window_offers_to_keep_the_login() {
        assert!(login_done_after_closing(false));
    }
}
