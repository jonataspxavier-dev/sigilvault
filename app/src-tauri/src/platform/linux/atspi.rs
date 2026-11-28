//! Acessibilidade do Linux (AT-SPI): é por onde o SigilVault descobre qual programa está em
//! primeiro plano, percebe que um campo de senha ganhou o foco e escreve nele. É o único jeito
//! que funciona no Wayland, onde um programa não pode espiar nem digitar em outro.
//! Linux accessibility (AT-SPI): this is how SigilVault finds which program is in the foreground,
//! notices a password field got focus and writes into it. It is the only way that works on
//! Wayland, where a program may neither spy on nor type into another.
//!
//! Funcionamento / How it works:
//! - Uma conexão com o barramento de acessibilidade escuta `object:state-changed:focused`. O
//!   último elemento que ganhou o foco (de outro programa) fica guardado.
//!   A connection to the accessibility bus listens to `object:state-changed:focused`. The last
//!   element that got focus (from another program) is kept.
//! - Escrever num campo usa `EditableText.SetTextContents`: não simula teclas, então não depende
//!   de a janela ter o foco do teclado e funciona no Wayland.
//!   Writing into a field uses `EditableText.SetTextContents`: it does not simulate keys, so it
//!   does not depend on the window having keyboard focus and works on Wayland.
//! - Nunca lemos o que a pessoa digita: só o papel do elemento que ganhou o foco (campo de senha ou não).
//!   We never read what the person types: only the role of the element that got focus (password field or not).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream};

/// Papel "campo de senha" do AT-SPI. / AT-SPI's "password field" role.
pub const ROLE_PASSWORD_TEXT: u32 = 40;
/// Quanto esperar uma resposta de outro programa (um programa travado não pode nos travar).
/// How long to wait for another program's answer (a hung program must not hang us).
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const REGISTRY: &str = "org.a11y.atspi.Registry";

/// Um elemento de outro programa: nome no barramento e caminho. / An element of another program: bus name and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub bus: String,
    pub path: OwnedObjectPath,
}

/// O que ganhou o foco. / What got focus.
#[derive(Debug, Clone)]
pub struct Focus {
    pub element: Element,
    pub pid: u32,
    pub role: u32,
    pub app_name: String,
    pub exe: PathBuf,
    /// Identifica este foco nas chamadas seguintes. / Identifies this focus in later calls.
    pub token: u64,
}

type Hook = Box<dyn Fn(&Focus) + Send + Sync>;

struct State {
    last: Option<Focus>,
    hooks: Vec<Hook>,
}

static STATE: Mutex<State> = Mutex::new(State { last: None, hooks: Vec::new() });
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static CONN: OnceLock<Connection> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    if RUNTIME.get().is_none() {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).thread_name("sigilvault-atspi").enable_all().build().ok()?;
        let _ = RUNTIME.set(rt);
    }
    RUNTIME.get()
}

/// O executor das chamadas de acessibilidade, para quem precisa de tarefas assíncronas.
/// The accessibility calls' executor, for those that need async tasks.
pub fn runtime_handle() -> Option<&'static tokio::runtime::Handle> {
    runtime().map(|rt| rt.handle())
}

/// Pede para ser avisado de cada novo foco. Chame antes de [`start`]. / Asks to be told of every new focus. Call before [`start`].
pub fn on_focus(hook: impl Fn(&Focus) + Send + Sync + 'static) {
    state().hooks.push(Box::new(hook));
}

/// O último foco de outro programa (nunca o do próprio SigilVault). / The last focus of another program (never SigilVault's own).
pub fn last_focus() -> Option<Focus> {
    state().last.clone()
}

/// Começa a escutar o foco. Pode ser chamado mais de uma vez. / Starts listening for focus. May be called more than once.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    if let Some(rt) = runtime() {
        rt.spawn(async {
            if let Some(conn) = connect().await {
                let _ = CONN.set(conn.clone());
                listen(conn).await;
            }
        });
    }
}

/// Conecta ao barramento de acessibilidade (o endereço vem do barramento da sessão).
/// Connects to the accessibility bus (the address comes from the session bus).
async fn connect() -> Option<Connection> {
    let session = Connection::session().await.ok()?;
    let reply = session.call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &()).await.ok()?;
    let address: String = reply.body().deserialize().ok()?;
    zbus::connection::Builder::address(address.as_str()).ok()?.build().await.ok()
}

async fn call<B: serde::Serialize + zbus::zvariant::DynamicType, T>(conn: &Connection, el: &Element, iface: &str, method: &str, body: &B) -> Option<T>
where
    T: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let fut = conn.call_method(Some(el.bus.as_str()), &el.path, Some(iface), method, body);
    let reply = tokio::time::timeout(CALL_TIMEOUT, fut).await.ok()?.ok()?;
    reply.body().deserialize().ok()
}

async fn pid_of(conn: &Connection, bus: &str) -> Option<u32> {
    let reply = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetConnectionUnixProcessID", &(bus,)).await.ok()?;
    reply.body().deserialize().ok()
}

/// Nome do programa dono do elemento (o nome da raiz da aplicação). / Name of the program that owns the element (the application root's name).
async fn app_name(conn: &Connection, el: &Element) -> String {
    let root = Element { bus: el.bus.clone(), path: OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").expect("caminho fixo / fixed path") };
    let reply = tokio::time::timeout(
        CALL_TIMEOUT,
        conn.call_method(Some(root.bus.as_str()), &root.path, Some("org.freedesktop.DBus.Properties"), "Get", &(ACCESSIBLE, "Name")),
    )
    .await;
    reply
        .ok()
        .and_then(Result::ok)
        .and_then(|r| r.body().deserialize::<OwnedValue>().ok())
        .and_then(|v| String::try_from(v).ok())
        .unwrap_or_default()
}

async fn listen(conn: Connection) {
    // Avisa o registro de que queremos esse evento (a forma com 3 argumentos é a nova; a de 2, a antiga).
    // Tells the registry we want this event (the 3-argument form is the new one; the 2-argument, the old).
    let registry = Some("org.a11y.atspi.Registry");
    let path = "/org/a11y/atspi/registry";
    let events: Vec<&str> = Vec::new();
    if conn.call_method(registry, path, Some(REGISTRY), "RegisterEvent", &("object:state-changed:focused", &events, "")).await.is_err() {
        let _ = conn.call_method(registry, path, Some(REGISTRY), "RegisterEvent", &("object:state-changed:focused", &events)).await;
    }
    let Ok(rule) = MatchRule::builder().msg_type(MessageType::Signal).interface("org.a11y.atspi.Event.Object").and_then(|b| b.member("StateChanged")) else { return };
    let Ok(mut stream) = MessageStream::for_match_rule(rule.build(), &conn, None).await else { return };
    while let Some(Ok(msg)) = stream.next().await {
        let header = msg.header();
        let (Some(sender), Some(path)) = (header.sender().map(|s| s.to_string()), header.path().map(|p| p.to_owned())) else { continue };
        let body = msg.body();
        // O formato do corpo mudou entre versões: tenta o novo e depois o antigo.
        // The body format changed between versions: tries the new one, then the old.
        let parsed = body
            .deserialize::<(String, i32, i32, OwnedValue, std::collections::HashMap<String, OwnedValue>)>()
            .map(|(kind, on, _, _, _)| (kind, on))
            .or_else(|_| body.deserialize::<(String, i32, i32, OwnedValue, (String, OwnedObjectPath))>().map(|(kind, on, _, _, _)| (kind, on)));
        let Ok((kind, gained)) = parsed else { continue };
        if kind != "focused" || gained != 1 {
            continue;
        }
        let element = Element { bus: sender, path: path.into() };
        let Some(pid) = pid_of(&conn, &element.bus).await else { continue };
        if pid == std::process::id() {
            continue;
        }
        let role = call::<_, u32>(&conn, &element, ACCESSIBLE, "GetRole", &()).await.unwrap_or(0);
        let Ok(exe) = sigilvault_ipc::process::image_path(pid) else { continue };
        let focus = Focus { app_name: app_name(&conn, &element).await, element, pid, role, exe, token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) };
        let mut st = state();
        st.last = Some(focus.clone());
        for hook in &st.hooks {
            hook(&focus);
        }
    }
}

/// Roda uma chamada assíncrona a partir de qualquer thread. / Runs an async call from any thread.
fn block<T>(fut: impl std::future::Future<Output = Option<T>>) -> Option<T> {
    runtime()?.handle().block_on(fut)
}

/// Escreve `text` no campo (substitui o que havia). / Writes `text` into the field (replacing what was there).
pub fn set_text(el: &Element, text: &str) -> bool {
    let Some(conn) = CONN.get() else { return false };
    block(call::<_, bool>(conn, el, "org.a11y.atspi.EditableText", "SetTextContents", &(text,))).unwrap_or(false)
}

/// Põe o foco no elemento. / Focuses the element.
pub fn grab_focus(el: &Element) -> bool {
    let Some(conn) = CONN.get() else { return false };
    block(call::<_, bool>(conn, el, "org.a11y.atspi.Component", "GrabFocus", &())).unwrap_or(false)
}

/// O elemento seguinte entre os irmãos (o "próximo campo" depois de um Tab).
/// The next element among siblings (the "next field" after a Tab).
pub fn next_sibling(el: &Element) -> Option<Element> {
    let conn = CONN.get()?;
    block(async {
        let index: i32 = call(conn, el, ACCESSIBLE, "GetIndexInParent", &()).await?;
        let (parent_bus, parent_path): (String, OwnedObjectPath) = call(conn, el, ACCESSIBLE, "GetParent", &()).await?;
        let parent = Element { bus: parent_bus, path: parent_path };
        let (bus, path): (String, OwnedObjectPath) = call(conn, &parent, ACCESSIBLE, "GetChildAtIndex", &(index + 1,)).await?;
        (path.as_str() != "/org/a11y/atspi/null").then_some(Element { bus, path })
    })
}
