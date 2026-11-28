//! Notificações do desktop (`org.freedesktop.Notifications`) com botões. É o "selo" do Wayland,
//! onde um programa não pode desenhar por cima do campo de outro.
//! Desktop notifications (`org.freedesktop.Notifications`) with buttons. It is Wayland's "seal",
//! where a program cannot draw over another program's field.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use zbus::message::Type as MessageType;
use zbus::zvariant::Value;
use zbus::{Connection, MatchRule, MessageStream};

type OnAction = Box<dyn Fn(&str) + Send + Sync>;

static CALLBACKS: Mutex<Option<HashMap<u32, OnAction>>> = Mutex::new(None);
static CONN: OnceLock<Connection> = OnceLock::new();
static LAST_ID: Mutex<u32> = Mutex::new(0);

/// Identificador da última notificação mostrada (para os testes). / ID of the last notification shown (for tests).
#[cfg(test)]
pub(super) fn last_id() -> u32 {
    *LAST_ID.lock().unwrap_or_else(|e| e.into_inner())
}

fn callbacks() -> std::sync::MutexGuard<'static, Option<HashMap<u32, OnAction>>> {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner())
}

async fn connection() -> Option<&'static Connection> {
    if let Some(c) = CONN.get() {
        return Some(c);
    }
    let conn = Connection::session().await.ok()?;
    let rule = MatchRule::builder().msg_type(MessageType::Signal).interface("org.freedesktop.Notifications").ok()?.member("ActionInvoked").ok()?.build();
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await.ok()?;
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let Ok((id, key)) = msg.body().deserialize::<(u32, String)>() else { continue };
            if let Some(callback) = callbacks().as_ref().and_then(|map| map.get(&id)) {
                callback(&key);
            }
        }
    });
    let _ = CONN.set(conn);
    CONN.get()
}

/// Mostra uma notificação com botões; `on_action` recebe a chave do botão apertado. Uma
/// notificação nova substitui a anterior (não empilha). Devolve se o desktop aceitou.
/// Shows a notification with buttons; `on_action` receives the pressed button's key. A new
/// notification replaces the previous one (does not stack). Returns whether the desktop accepted it.
pub async fn show(summary: &str, body: &str, actions: &[(&str, &str)], on_action: impl Fn(&str) + Send + Sync + 'static) -> bool {
    let Some(conn) = connection().await else { return false };
    let flat: Vec<&str> = actions.iter().flat_map(|(key, label)| [*key, *label]).collect();
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("desktop-entry", Value::from("SigilVault"));
    hints.insert("transient", Value::from(true));
    let replaces = *LAST_ID.lock().unwrap_or_else(|e| e.into_inner());
    let args = ("SigilVault", replaces, "dialog-password", summary, body, flat, hints, 10_000i32);
    let call = conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &args,
    );
    let Ok(Ok(reply)) = tokio::time::timeout(Duration::from_secs(3), call).await else { return false };
    let Ok(id) = reply.body().deserialize::<u32>() else { return false };
    *LAST_ID.lock().unwrap_or_else(|e| e.into_inner()) = id;
    callbacks().get_or_insert_with(HashMap::new).insert(id, Box::new(on_action));
    true
}
