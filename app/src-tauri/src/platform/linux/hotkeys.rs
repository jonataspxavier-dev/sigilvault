//! Atalhos globais pelo portal `GlobalShortcuts` do desktop. O Wayland não deixa um programa
//! escutar teclas de outros, então o desktop é quem registra a combinação e avisa quando ela é
//! usada (na primeira vez, ele pergunta à pessoa).
//! Global hotkeys through the desktop's `GlobalShortcuts` portal. Wayland does not let a program
//! listen to other programs' keys, so the desktop registers the combination and tells us when it
//! is used (the first time, it asks the person).

use super::target::Target;

/// O que cada atalho faz. / What each hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Pop-up de busca (copiar ou digitar). / Search popup (copy or type).
    Search = 1,
    /// "Gerar ou só salvar" para programas. / "Generate or just save" for programs.
    Generate = 2,
}

const CTRL: u8 = 1;
const ALT: u8 = 2;
const SHIFT: u8 = 4;
const SUPER: u8 = 8;

/// Combinação de teclas. / Key combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    mods: u8,
    /// Nome canônico da tecla: `Space`, `F5`, `K`, `7`. / Canonical key name.
    key: String,
}

/// Lê "Ctrl+Shift+Espaço", "Alt+F2", "Super+K"… Exige Ctrl, Alt ou Super (Shift sozinho roubaria
/// letras maiúsculas). "Win" e "Meta" valem como Super.
/// Reads "Ctrl+Shift+Space", "Alt+F2", "Super+K"… Requires Ctrl, Alt or Super (Shift alone would
/// steal capital letters). "Win" and "Meta" count as Super.
pub fn parse(text: &str) -> Option<Combo> {
    let mut mods = 0u8;
    let mut key = None;
    for raw in text.split('+') {
        let part = raw.trim().to_lowercase();
        match part.as_str() {
            "ctrl" | "control" => mods |= CTRL,
            "alt" => mods |= ALT,
            "shift" => mods |= SHIFT,
            "super" | "win" | "meta" => mods |= SUPER,
            "space" | "espaço" | "espaco" => key = Some("Space".to_owned()),
            p if p.len() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric()) => key = Some(p.to_ascii_uppercase()),
            p if p.starts_with('f') => match p[1..].parse::<u32>() {
                Ok(n @ 1..=24) => key = Some(format!("F{n}")),
                _ => return None,
            },
            _ => return None,
        }
    }
    let key = key?;
    if mods & (CTRL | ALT | SUPER) == 0 {
        return None;
    }
    Some(Combo { mods, key })
}

/// Texto canônico da combinação. / Canonical text of the combination.
pub fn format(c: Combo) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (flag, name) in [(CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift"), (SUPER, "Super")] {
        if c.mods & flag != 0 {
            parts.push(name);
        }
    }
    parts.push(&c.key);
    parts.join("+")
}

/// Texto da combinação no formato do portal (`CTRL+SHIFT+space`): modificadores em maiúsculas e
/// a tecla pelo nome do xkb. / Combination text in the portal's format (`CTRL+SHIFT+space`):
/// uppercase modifiers and the key by its xkb name.
fn portal_trigger(c: &Combo) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (flag, name) in [(CTRL, "CTRL"), (ALT, "ALT"), (SHIFT, "SHIFT"), (SUPER, "LOGO")] {
        if c.mods & flag != 0 {
            parts.push(name.into());
        }
    }
    parts.push(if c.key.len() == 1 || c.key == "Space" { c.key.to_lowercase() } else { c.key.clone() });
    parts.join("+")
}

fn action_id(a: Action) -> &'static str {
    match a {
        Action::Search => "search",
        Action::Generate => "generate",
    }
}

fn action_from_id(id: &str) -> Option<Action> {
    match id {
        "search" => Some(Action::Search),
        "generate" => Some(Action::Generate),
        _ => None,
    }
}

type Handler = Box<dyn Fn(Action, Option<Target>) + Send + Sync>;
type Request = (Vec<(Action, Option<Combo>)>, std::sync::mpsc::Sender<Vec<(Action, bool)>>);

static HANDLER: std::sync::OnceLock<Handler> = std::sync::OnceLock::new();
static REQUESTS: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<Request>> = std::sync::OnceLock::new();

/// Identificador do app para os portais (igual ao do pacote). / App identifier for the portals (same as the package's).
const APP_ID: &str = "app.sigilvault.desktop";
/// O portal pode mostrar uma janela de permissão: espera a pessoa. / The portal may show a permission window: waits for the person.
const BIND_WAIT: std::time::Duration = std::time::Duration::from_secs(90);

/// Inicia a escuta dos atalhos. `on_press` recebe a ação e a janela que estava ativa. No Wayland o
/// atalho passa pelo portal `GlobalShortcuts` do desktop (o GNOME 48+ e o KDE têm); se o desktop
/// não tiver, nenhum atalho é aceito e a pessoa usa `sigilvault-app --quick search`.
/// Starts listening for hotkeys. `on_press` gets the action and the window that was active. On
/// Wayland the hotkey goes through the desktop's `GlobalShortcuts` portal (GNOME 48+ and KDE have
/// it); if the desktop lacks it, no hotkey is accepted and the person uses `sigilvault-app --quick search`.
pub fn start(on_press: impl Fn(Action, Option<Target>) + Send + Sync + 'static) {
    if HANDLER.set(Box::new(on_press)).is_err() {
        return;
    }
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = REQUESTS.set(tx);
    let _ = std::thread::Builder::new().name("sigilvault-hotkeys".into()).spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        runtime.block_on(portal_loop(rx));
    });
}

/// Troca os atalhos. Devolve, para cada um, se o portal o aceitou. `None` desliga o atalho.
/// Replaces the hotkeys. Returns, for each one, whether the portal accepted it. `None` turns it off.
pub fn apply(bindings: Vec<(Action, Option<Combo>)>) -> Vec<(Action, bool)> {
    let fallback = |b: &[(Action, Option<Combo>)]| b.iter().map(|(a, c)| (*a, c.is_none())).collect::<Vec<_>>();
    let Some(requests) = REQUESTS.get() else { return fallback(&bindings) };
    let (tx, rx) = std::sync::mpsc::channel();
    let answer_for = bindings.clone();
    if requests.send((bindings, tx)).is_err() {
        return fallback(&answer_for);
    }
    rx.recv_timeout(BIND_WAIT + std::time::Duration::from_secs(5)).unwrap_or_else(|_| fallback(&answer_for))
}

async fn portal_loop(mut requests: tokio::sync::mpsc::UnboundedReceiver<Request>) {
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use futures_util::StreamExt;

    let mut portal: Option<(GlobalShortcuts, ashpd::desktop::Session<GlobalShortcuts>)> = None;
    let mut activated: Option<std::pin::Pin<Box<dyn futures_util::Stream<Item = ashpd::desktop::global_shortcuts::Activated> + Send>>> = None;
    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some((bindings, reply)) = request else { return };
                let wanted: Vec<_> = bindings.iter().filter_map(|(a, c)| c.as_ref().map(|c| (*a, portal_trigger(c)))).collect();
                let mut accepted: Vec<Action> = Vec::new();
                if !wanted.is_empty() {
                    if portal.is_none() {
                        // Sem isso o portal não sabe quem somos (app fora de sandbox). / Without this the portal does not know who we are (non-sandboxed app).
                        if let Ok(id) = APP_ID.try_into() {
                            let _ = ashpd::register_host_app(id).await;
                        }
                        if let Ok(proxy) = GlobalShortcuts::new().await
                            && let Ok(session) = proxy.create_session(CreateSessionOptions::default()).await
                        {
                            activated = proxy.receive_activated().await.ok().map(|s| Box::pin(s) as _);
                            portal = Some((proxy, session));
                        }
                    }
                    if let Some((proxy, session)) = portal.as_ref() {
                        let shortcuts: Vec<NewShortcut> = wanted
                            .iter()
                            .map(|(a, trigger)| {
                                let text = if *a == Action::Search { "Abrir a busca do SigilVault / Open SigilVault search" } else { "Gerar ou salvar senha / Generate or save a password" };
                                NewShortcut::new(action_id(*a), text).preferred_trigger(trigger.as_str())
                            })
                            .collect();
                        let bound = tokio::time::timeout(BIND_WAIT, async {
                            proxy.bind_shortcuts(session, &shortcuts, None, BindShortcutsOptions::default()).await?.response()
                        })
                        .await;
                        if let Ok(Ok(response)) = bound {
                            accepted = response.shortcuts().iter().filter_map(|s| action_from_id(s.id())).collect();
                        }
                    }
                }
                let answer = bindings.iter().map(|(a, c)| (*a, c.is_none() || accepted.contains(a))).collect();
                let _ = reply.send(answer);
            }
            Some(event) = async {
                match activated.as_mut() {
                    Some(stream) => stream.next().await,
                    None => std::future::pending().await,
                }
            } => {
                if let (Some(action), Some(handler)) = (action_from_id(event.shortcut_id()), HANDLER.get()) {
                    // Lê a janela ativa antes de qualquer pop-up existir. / Reads the active window before any popup exists.
                    handler(action, super::target::foreground());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_combinations() {
        assert_eq!(format(parse("ctrl + shift + espaço").unwrap()), "Ctrl+Shift+Space");
        assert_eq!(format(parse("Alt+F2").unwrap()), "Alt+F2");
        assert_eq!(format(parse("Ctrl+Alt+k").unwrap()), "Ctrl+Alt+K");
        assert_eq!(format(parse("Win+7").unwrap()), "Super+7");
        for bad in ["Shift+A", "A", "Ctrl+Shift+Enter", "Ctrl+F25", "Ctrl+Shift", "", "Ctrl+AB"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn builds_the_portal_trigger() {
        assert_eq!(portal_trigger(&parse("Ctrl+Shift+Espaço").unwrap()), "CTRL+SHIFT+space");
        assert_eq!(portal_trigger(&parse("Ctrl+Alt+k").unwrap()), "CTRL+ALT+k");
        assert_eq!(portal_trigger(&parse("Super+F5").unwrap()), "LOGO+F5");
        assert_eq!(portal_trigger(&parse("Win+7").unwrap()), "LOGO+7");
        assert_eq!(action_from_id(action_id(Action::Generate)), Some(Action::Generate));
        assert_eq!(action_from_id("x"), None);
    }
}
