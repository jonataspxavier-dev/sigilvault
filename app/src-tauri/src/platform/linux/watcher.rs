//! Bloqueio por eventos do sistema: tela bloqueada, suspensão e fim da sessão.
//! Lock on system events: screen locked, suspend and session end.
//!
//! Escuta dois barramentos D-Bus / Listens on two D-Bus buses:
//! - **sistema:** o `logind` avisa `Lock` da nossa sessão, `PrepareForSleep` (suspensão) e
//!   `SessionRemoved` (fim da sessão). / **system:** `logind` reports `Lock` for our session,
//!   `PrepareForSleep` (suspend) and `SessionRemoved` (session end).
//! - **sessão:** o protetor de tela do GNOME e o do freedesktop (KDE e outros) avisam
//!   `ActiveChanged`. / **session:** the GNOME and freedesktop (KDE and others) screensavers
//!   report `ActiveChanged`.

use std::sync::Arc;

use futures_util::StreamExt;
use zbus::message::Type as MessageType;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, MessageStream};

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";

/// Este sinal manda trancar o cofre? `flag` é o booleano do sinal, quando há um.
/// Does this signal mean the vault must lock? `flag` is the signal's boolean, when it has one.
fn is_lock_event(member: &str, flag: Option<bool>) -> bool {
    match member {
        // `Lock` e `SessionRemoved` já chegam filtrados para a nossa sessão. / Already filtered to our session.
        "Lock" | "SessionRemoved" => true,
        // Suspender ou ligar o protetor de tela; "false" é o fim disso. / Suspending or screensaver on; "false" is the end of it.
        "PrepareForSleep" | "ActiveChanged" => flag == Some(true),
        _ => false,
    }
}

fn rule(interface: &'static str, member: &'static str, path: Option<&str>) -> Option<MatchRule<'static>> {
    let mut builder = MatchRule::builder().msg_type(MessageType::Signal).interface(interface).ok()?.member(member).ok()?;
    if let Some(path) = path {
        builder = builder.path(path.to_owned()).ok()?;
    }
    Some(builder.build())
}

/// Chama `on_lock` para cada sinal de bloqueio que chegar pelas regras dadas.
/// Calls `on_lock` for every lock signal that arrives through the given rules.
async fn listen(conn: &Connection, rules: Vec<MatchRule<'static>>, our_sessions: Vec<OwnedObjectPath>, on_lock: Arc<dyn Fn() + Send + Sync>) {
    let mut streams = Vec::new();
    for rule in rules {
        if let Ok(stream) = MessageStream::for_match_rule(rule, conn, None).await {
            streams.push(stream);
        }
    }
    let mut merged = futures_util::stream::select_all(streams);
    while let Some(Ok(msg)) = merged.next().await {
        let header = msg.header();
        let Some(member) = header.member().map(|m| m.as_str().to_owned()) else { continue };
        // `SessionRemoved` vale para qualquer sessão: só a nossa conta. / `SessionRemoved` covers any session: only ours counts.
        if member == "SessionRemoved" {
            let removed = msg.body().deserialize::<(String, OwnedObjectPath)>().ok().map(|(_, path)| path);
            if !removed.is_some_and(|path| our_sessions.contains(&path)) {
                continue;
            }
        }
        let flag = msg.body().deserialize::<bool>().ok();
        if is_lock_event(&member, flag) {
            on_lock();
        }
    }
}

/// Caminhos das nossas sessões no logind. Pelo PID; se o processo não estiver numa sessão (um
/// terminal aberto fora do login, por exemplo), pelo `XDG_SESSION_ID`; e, por último, todas as
/// sessões gráficas do nosso usuário (trancar a mais é o lado seguro).
/// Paths of our sessions in logind. By PID; if the process is not in a session (a terminal opened
/// outside the login, for instance), by `XDG_SESSION_ID`; and lastly all graphical sessions of our
/// user (locking too much is the safe side).
async fn our_sessions(conn: &Connection) -> Vec<OwnedObjectPath> {
    let manager = Some("org.freedesktop.login1.Manager");
    if let Ok(reply) = conn.call_method(Some(LOGIND), LOGIND_PATH, manager, "GetSessionByPID", &(std::process::id())).await
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
    {
        return vec![path];
    }
    if let Some(id) = std::env::var("XDG_SESSION_ID").ok().filter(|id| !id.is_empty())
        && let Ok(reply) = conn.call_method(Some(LOGIND), LOGIND_PATH, manager, "GetSession", &(id.as_str())).await
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
    {
        return vec![path];
    }
    // SAFETY: `geteuid` não recebe argumentos e não falha. / `geteuid` takes no arguments and cannot fail.
    let uid = unsafe { libc::geteuid() };
    let Ok(reply) = conn.call_method(Some(LOGIND), LOGIND_PATH, manager, "ListSessions", &()).await else { return Vec::new() };
    let all: Vec<(String, u32, String, String, OwnedObjectPath)> = reply.body().deserialize().unwrap_or_default();
    all.into_iter().filter(|(_, owner, _, seat, _)| *owner == uid && !seat.is_empty()).map(|(_, _, _, _, path)| path).collect()
}

/// logind: nossa sessão, suspensão e fim da sessão. / logind: our session, suspend and session end.
async fn watch_system(on_lock: Arc<dyn Fn() + Send + Sync>) {
    let Ok(conn) = Connection::system().await else { return };
    let sessions = our_sessions(&conn).await;
    let mut rules = vec![
        rule("org.freedesktop.login1.Manager", "PrepareForSleep", Some(LOGIND_PATH)),
        rule("org.freedesktop.login1.Manager", "SessionRemoved", Some(LOGIND_PATH)),
    ];
    for session in &sessions {
        rules.push(rule("org.freedesktop.login1.Session", "Lock", Some(session.as_str())));
    }
    listen(&conn, rules.into_iter().flatten().collect(), sessions, on_lock).await;
}

/// Protetor de tela do GNOME e do freedesktop. / GNOME and freedesktop screensavers.
async fn watch_session(on_lock: Arc<dyn Fn() + Send + Sync>) {
    let Ok(conn) = Connection::session().await else { return };
    let rules = [rule("org.gnome.ScreenSaver", "ActiveChanged", None), rule("org.freedesktop.ScreenSaver", "ActiveChanged", None)];
    listen(&conn, rules.into_iter().flatten().collect(), Vec::new(), on_lock).await;
}

/// Começa a vigiar. `on_lock` roda a cada evento que deve trancar o cofre.
/// Starts watching. `on_lock` runs on every event that must lock the vault.
pub fn start(on_lock: impl Fn() + Send + Sync + 'static) {
    let on_lock: Arc<dyn Fn() + Send + Sync> = Arc::new(on_lock);
    let _ = std::thread::Builder::new().name("sigilvault-watcher".into()).spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        runtime.block_on(async {
            tokio::join!(watch_system(on_lock.clone()), watch_session(on_lock));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decides_which_signals_lock() {
        assert!(is_lock_event("Lock", None));
        assert!(is_lock_event("SessionRemoved", None));
        assert!(is_lock_event("PrepareForSleep", Some(true)));
        assert!(!is_lock_event("PrepareForSleep", Some(false)));
        assert!(is_lock_event("ActiveChanged", Some(true)));
        assert!(!is_lock_event("ActiveChanged", Some(false)));
        assert!(!is_lock_event("ActiveChanged", None));
        assert!(!is_lock_event("Unlock", None));
    }

    /// Fala com o D-Bus da sessão de verdade: rode com `cargo test -- --ignored`. Emite um sinal de
    /// protetor de tela falso (não tranca a tela de ninguém).
    /// Talks to the real session D-Bus: run with `cargo test -- --ignored`. Emits a fake screensaver
    /// signal (it does not lock anyone's screen).
    #[test]
    #[ignore = "usa o D-Bus da sessão / uses the session D-Bus"]
    fn reacts_to_a_screensaver_signal() {
        let (tx, rx) = std::sync::mpsc::channel();
        start(move || {
            let _ = tx.send(());
        });
        std::thread::sleep(std::time::Duration::from_millis(1500));
        // `false` (tela destravada) não pode trancar; `true` (tela travada) precisa. / `false` (unlocked) must not lock; `true` (locked) must.
        let emit = |value: &str| {
            let status = std::process::Command::new("gdbus")
                .args(["emit", "--session", "--object-path", "/org/gnome/ScreenSaver", "--signal", "org.gnome.ScreenSaver.ActiveChanged", value])
                .status()
                .unwrap();
            assert!(status.success());
        };
        emit("false");
        assert!(rx.recv_timeout(std::time::Duration::from_millis(800)).is_err());
        emit("true");
        assert!(rx.recv_timeout(std::time::Duration::from_secs(3)).is_ok());
    }

    #[test]
    fn match_rules_build() {
        assert!(rule("org.freedesktop.login1.Manager", "PrepareForSleep", Some(LOGIND_PATH)).is_some());
        assert!(rule("org.gnome.ScreenSaver", "ActiveChanged", None).is_some());
    }
}
