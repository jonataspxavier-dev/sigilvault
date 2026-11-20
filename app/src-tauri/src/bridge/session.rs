//! Uma conexão com a extensão (através do host). / One connection with the extension (through the host).
//!
//! Mensagens em claro (antes do canal) / Plain messages (before the channel):
//! - `{"t":"hello"}` → `{"t":"hello","v":1}`
//! - `{"t":"pair_start","browser":"…"}` → `{"t":"pair_started","expiresIn":120}`
//! - `{"t":"pair_login1","req":hex}` → `{"t":"pair_login1","resp":hex}`
//! - `{"t":"pair_login2","fin":hex}` → `{"t":"paired","id":hex}`
//! - `{"t":"auth","id":hex,"n":hex}` → `{"t":"auth","n":hex}` (abre o canal / opens the channel)
//! - erros / errors: `{"t":"error","code":"…"}`
//!
//! Depois do `auth`, só `{"t":"box","c":hex}` (AES-256-GCM). Dentro: `{"op":"status"|"open_app"|"unpair","rid":n}`.
//! After `auth`, only `{"t":"box","c":hex}` (AES-256-GCM). Inside: `{"op":…,"rid":n}`.
//! O app também envia avisos: `{"ev":"status",…}` e `{"ev":"unpaired"}`.
//! The app also sends notices: `{"ev":"status",…}` and `{"ev":"unpaired"}`.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sigilvault_core::hex;
use sigilvault_ipc::pipe::Pipe;
use sigilvault_ipc::{HOST_EXE, MAX_MESSAGE, allowed_caller, frame, process, signature};
use sigilvault_pairing::Channel;
use tauri::AppHandle;

use crate::window;

/// Limite de mensagens por janela de tempo. / Message limit per time window.
const RATE_WINDOW: Duration = Duration::from_secs(10);
const RATE_MAX: u32 = 40;

struct Out {
    channel: Option<Channel>,
    pairing: Option<[u8; 16]>,
}

/// Lado do app de uma conexão. Todas as escritas passam pela mesma trava, para o contador do
/// canal seguir a ordem do pipe. / App side of a connection. All writes go through the same lock,
/// so the channel counter follows the pipe order.
pub struct Conn {
    pub id: u64,
    /// Origem da extensão, segundo o navegador. / Extension origin, according to the browser.
    pub caller: String,
    pipe: Pipe,
    out: Mutex<Out>,
}

impl Conn {
    fn out(&self) -> MutexGuard<'_, Out> {
        self.out.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn send_plain(&self, msg: &Value) -> bool {
        let _guard = self.out();
        frame::write(&self.pipe, msg.to_string().as_bytes(), MAX_MESSAGE).is_ok()
    }

    /// Envia criptografado (só depois do `auth`). / Sends encrypted (only after `auth`).
    pub fn send_box(&self, msg: &Value) -> bool {
        let mut out = self.out();
        let Some(channel) = out.channel.as_mut() else { return false };
        let sealed = channel.seal(msg.to_string().as_bytes());
        let wire = json!({ "t": "box", "c": hex::encode(&sealed) }).to_string();
        frame::write(&self.pipe, wire.as_bytes(), MAX_MESSAGE).is_ok()
    }

    fn open_box(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        self.out().channel.as_mut()?.open(sealed).ok()
    }

    /// Liga o canal e responde com o nonce do app, de forma atômica. Falha se já estava ligado.
    /// Turns the channel on and replies with the app nonce, atomically. Fails if already on.
    pub fn authenticate(&self, channel: Channel, pairing: [u8; 16], app_nonce: &[u8; 32]) -> bool {
        let mut out = self.out();
        if out.channel.is_some() {
            return false;
        }
        let reply = json!({ "t": "auth", "n": hex::encode(app_nonce) }).to_string();
        if frame::write(&self.pipe, reply.as_bytes(), MAX_MESSAGE).is_err() {
            return false;
        }
        out.channel = Some(channel);
        out.pairing = Some(pairing);
        true
    }

    pub fn pairing_id(&self) -> Option<[u8; 16]> {
        self.out().pairing
    }

    /// Encerra a conexão (destrava a leitura pendente). / Ends the connection (unblocks the pending read).
    pub fn close(&self) {
        self.pipe.cancel();
    }
}

fn error(code: &str) -> Value {
    json!({ "t": "error", "code": code })
}

/// Lê um campo hexadecimal de tamanho fixo. / Reads a fixed-size hex field.
fn hex_field<const N: usize>(msg: &Value, key: &str) -> Option<[u8; N]> {
    hex::decode(msg.get(key)?.as_str()?)?.try_into().ok()
}

/// Lê um campo hexadecimal de tamanho livre (limitado). / Reads a variable-size hex field (bounded).
fn hex_bytes(msg: &Value, key: &str) -> Option<Vec<u8>> {
    let s = msg.get(key)?.as_str()?;
    if s.len() > 8 * 1024 {
        return None;
    }
    hex::decode(s)
}

/// O executável do outro lado é o nosso host? / Is the executable on the other side our host?
fn peer_is_our_host(pipe: &Pipe) -> bool {
    let (Ok(me), Ok(host)) = (std::env::current_exe(), process::sibling(HOST_EXE)) else { return false };
    pipe.peer_pid()
        .and_then(process::image_path)
        .is_ok_and(|peer| process::same_file(&peer, &host) && signature::peer_allowed(&me, &peer))
}

/// Atende uma conexão até ela terminar. / Serves one connection until it ends.
pub fn serve(app: &AppHandle, pipe: Pipe, id: u64) {
    if !peer_is_our_host(&pipe) {
        return;
    }
    // A primeira mensagem vem do próprio host: quem chamou, segundo o navegador.
    // The first message comes from the host itself: who called, according to the browser.
    let Ok(Some(first)) = frame::read(&pipe, MAX_MESSAGE) else { return };
    let header: Value = serde_json::from_slice(&first).unwrap_or(Value::Null);
    let caller = header.get("caller").and_then(Value::as_str).unwrap_or_default().to_owned();
    if header.get("t") != Some(&json!("host")) || !allowed_caller(&caller) {
        let _ = frame::write(&pipe, error("unknown_extension").to_string().as_bytes(), MAX_MESSAGE);
        return;
    }

    let conn = Arc::new(Conn { id, caller, pipe, out: Mutex::new(Out { channel: None, pairing: None }) });
    let mut window_start = Instant::now();
    let mut count = 0u32;
    while let Ok(Some(raw)) = frame::read(&conn.pipe, MAX_MESSAGE) {
        if window_start.elapsed() > RATE_WINDOW {
            window_start = Instant::now();
            count = 0;
        }
        count += 1;
        if count > RATE_MAX {
            conn.send_plain(&error("rate_limited"));
            break;
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&raw) else { break };
        if !handle(app, &conn, &msg) {
            break;
        }
    }
}

/// Trata uma mensagem. `false` = encerrar a conexão. / Handles one message. `false` = end the connection.
fn handle(app: &AppHandle, conn: &Arc<Conn>, msg: &Value) -> bool {
    let kind = msg.get("t").and_then(Value::as_str).unwrap_or_default();
    let authenticated = conn.out().channel.is_some();
    if authenticated && kind != "box" {
        // Depois do canal, nada em claro. / After the channel, nothing in clear.
        return false;
    }
    let reply = match kind {
        "hello" => json!({ "t": "hello", "v": 1 }),
        "pair_start" => {
            let browser = msg.get("browser").and_then(Value::as_str).unwrap_or_default();
            match super::start_pairing(app, conn, browser) {
                Ok(secs) => json!({ "t": "pair_started", "expiresIn": secs }),
                Err(code) => error(code),
            }
        }
        "pair_login1" => match hex_bytes(msg, "req") {
            None => error("bad_message"),
            Some(req) => match super::pair_login1(app, conn, &req) {
                Ok(resp) => json!({ "t": "pair_login1", "resp": hex::encode(&resp) }),
                Err(code) => error(code),
            },
        },
        "pair_login2" => match hex_bytes(msg, "fin") {
            None => error("bad_message"),
            Some(fin) => match super::pair_login2(app, conn, &fin) {
                Ok(id) => json!({ "t": "paired", "id": hex::encode(&id) }),
                Err(code) => error(code),
            },
        },
        "auth" => match (hex_field::<16>(msg, "id"), hex_field::<32>(msg, "n")) {
            (Some(id), Some(nonce)) => match super::authenticate(app, conn, &id, &nonce) {
                // A resposta já saiu junto com a ligação do canal. / The reply already went out with the channel.
                Ok(()) => return true,
                Err(code) => error(code),
            },
            _ => error("bad_message"),
        },
        "box" if authenticated => return handle_box(app, conn, msg),
        "box" => error("not_authenticated"),
        _ => error("unknown_message"),
    };
    conn.send_plain(&reply)
}

/// Mensagem criptografada. Falha ao abrir = encerrar (adulteração ou fora de ordem).
/// Encrypted message. Failing to open = end (tampering or out of order).
fn handle_box(app: &AppHandle, conn: &Arc<Conn>, msg: &Value) -> bool {
    let Some(sealed) = hex_bytes(msg, "c") else { return false };
    let Some(plain) = conn.open_box(&sealed) else { return false };
    let Ok(req) = serde_json::from_slice::<Value>(&plain) else { return false };
    let rid = req.get("rid").cloned().unwrap_or(Value::Null);
    let mut reply = match req.get("op").and_then(Value::as_str).unwrap_or_default() {
        "status" => super::status(app),
        "open_app" => {
            window::open(app);
            json!({ "ok": true })
        }
        "unpair" => {
            let Some(id) = conn.pairing_id() else { return false };
            let ok = super::remove_pairing(app, &id).is_ok();
            conn.send_box(&json!({ "rid": rid, "ok": ok }));
            return false;
        }
        _ => json!({ "error": "unknown_op" }),
    };
    reply["rid"] = rid;
    conn.send_box(&reply)
}
