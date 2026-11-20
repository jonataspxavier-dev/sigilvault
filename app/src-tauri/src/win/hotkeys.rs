//! Atalhos globais (RegisterHotKey). O Windows avisa se a combinação já é de outro programa, e a
//! janela-alvo é capturada no instante da tecla, antes de o pop-up abrir.
//! Global hotkeys (RegisterHotKey). Windows reports if the combination already belongs to another
//! program, and the target window is captured at key time, before the popup opens.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY};

use super::target::{self, Target};

/// O que cada atalho faz. / What each hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Pop-up de busca (copiar ou digitar). / Search popup (copy or type).
    Search = 1,
    /// "Gerar ou só salvar" para programas. / "Generate or just save" for programs.
    Generate = 2,
}

impl Action {
    fn from_id(id: usize) -> Option<Self> {
        match id {
            1 => Some(Self::Search),
            2 => Some(Self::Generate),
            _ => None,
        }
    }
}

/// Combinação de teclas. / Key combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub mods: u32,
    pub vk: u32,
}

/// Lê "Ctrl+Shift+Espaço", "Alt+F2", "Ctrl+Alt+K"… Exige Ctrl, Alt ou Win (Shift sozinho roubaria
/// letras maiúsculas). / Reads "Ctrl+Shift+Space", "Alt+F2", "Ctrl+Alt+K"… Requires Ctrl, Alt or
/// Win (Shift alone would steal capital letters).
pub fn parse(text: &str) -> Option<Combo> {
    let mut mods = 0u32;
    let mut vk = None;
    for raw in text.split('+') {
        let part = raw.trim().to_lowercase();
        match part.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" => mods |= MOD_WIN,
            "space" | "espaço" | "espaco" => vk = Some(0x20),
            p if p.len() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric()) => vk = Some(u32::from(p.to_ascii_uppercase().as_bytes()[0])),
            p if p.starts_with('f') => match p[1..].parse::<u32>() {
                Ok(n @ 1..=24) => vk = Some(0x6F + n),
                _ => return None,
            },
            _ => return None,
        }
    }
    let vk = vk?;
    if mods & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 {
        return None;
    }
    Some(Combo { mods, vk })
}

/// Texto canônico da combinação. / Canonical text of the combination.
pub fn format(c: Combo) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (flag, name) in [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift"), (MOD_WIN, "Win")] {
        if c.mods & flag != 0 {
            parts.push(name.into());
        }
    }
    parts.push(match c.vk {
        0x20 => "Space".into(),
        0x70..=0x87 => format!("F{}", c.vk - 0x6F),
        v => char::from_u32(v).map(String::from).unwrap_or_default(),
    });
    parts.join("+")
}

type Handler = Box<dyn Fn(Action, Option<Target>) + Send + Sync>;
type Request = (Vec<(Action, Option<Combo>)>, mpsc::Sender<Vec<(Action, bool)>>);

static THREAD: AtomicU32 = AtomicU32::new(0);
static HANDLER: OnceLock<Handler> = OnceLock::new();
static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
const WM_APPLY: u32 = WM_APP + 1;

/// Inicia a thread dos atalhos. `on_press` recebe a ação e a janela que estava ativa.
/// Starts the hotkey thread. `on_press` gets the action and the window that was active.
pub fn start(on_press: impl Fn(Action, Option<Target>) + Send + Sync + 'static) {
    if HANDLER.set(Box::new(on_press)).is_err() {
        return;
    }
    let _ = std::thread::Builder::new().name("sigilvault-hotkeys".into()).spawn(|| {
        // SAFETY: fila de mensagens desta thread; os atalhos são registrados e removidos nela.
        // SAFETY: this thread's message queue; hotkeys are registered and removed on it.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            // Cria a fila antes de publicar o id da thread. / Creates the queue before publishing the thread id.
            PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            THREAD.store(GetCurrentThreadId(), Ordering::Release);
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                match msg.message {
                    WM_HOTKEY => {
                        if let (Some(action), Some(handler)) = (Action::from_id(msg.wParam), HANDLER.get()) {
                            // A janela ativa AGORA, antes de qualquer pop-up. / The active window NOW, before any popup.
                            handler(action, target::foreground());
                        }
                    }
                    WM_APPLY => {
                        let Some((bindings, reply)) = REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
                        let mut results = Vec::new();
                        for (action, combo) in bindings {
                            UnregisterHotKey(std::ptr::null_mut(), action as i32);
                            let ok = match combo {
                                Some(c) => RegisterHotKey(std::ptr::null_mut(), action as i32, c.mods | MOD_NOREPEAT, c.vk) != 0,
                                None => true,
                            };
                            results.push((action, ok));
                        }
                        let _ = reply.send(results);
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Troca os atalhos. Devolve, para cada um, se o Windows aceitou (false = já em uso por outro
/// programa). `None` desliga o atalho. / Replaces the hotkeys. Returns, for each one, whether
/// Windows accepted it (false = already in use by another program). `None` turns it off.
pub fn apply(bindings: Vec<(Action, Option<Combo>)>) -> Vec<(Action, bool)> {
    let mut thread = 0;
    for _ in 0..100 {
        thread = THREAD.load(Ordering::Acquire);
        if thread != 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if thread == 0 {
        return bindings.into_iter().map(|(a, _)| (a, false)).collect();
    }
    let actions: Vec<Action> = bindings.iter().map(|(a, _)| *a).collect();
    let (tx, rx) = mpsc::channel();
    *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some((bindings, tx));
    // SAFETY: id de uma thread nossa, com fila criada. / Id of one of our threads, with its queue created.
    unsafe { PostThreadMessageW(thread, WM_APPLY, 0, 0) };
    rx.recv_timeout(Duration::from_secs(3)).unwrap_or_else(|_| actions.into_iter().map(|a| (a, false)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_combinations() {
        let c = parse("ctrl + shift + espaço").unwrap();
        assert_eq!(c, Combo { mods: MOD_CONTROL | MOD_SHIFT, vk: 0x20 });
        assert_eq!(format(c), "Ctrl+Shift+Space");
        assert_eq!(format(parse("Alt+F2").unwrap()), "Alt+F2");
        assert_eq!(format(parse("Ctrl+Alt+k").unwrap()), "Ctrl+Alt+K");
        assert_eq!(parse("Win+7").unwrap().vk, u32::from(b'7'));
        // Sem Ctrl/Alt/Win, tecla desconhecida, F25, sem tecla. / No Ctrl/Alt/Win, unknown key, F25, no key.
        for bad in ["Shift+A", "A", "Ctrl+Shift+Enter", "Ctrl+F25", "Ctrl+Shift", "", "Ctrl+AB"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn registers_and_detects_combinations_in_use() {
        start(|_, _| {});
        // Uma combinação improvável para não brigar com outros programas. / An unlikely combination.
        let combo = parse("Ctrl+Alt+Shift+F23").unwrap();
        assert_eq!(apply(vec![(Action::Search, Some(combo))]), vec![(Action::Search, true)]);
        // Outra thread tentando a mesma combinação: o Windows recusa (já está em uso).
        // Another thread trying the same combination: Windows refuses (already in use).
        let taken = std::thread::spawn(move || unsafe { RegisterHotKey(std::ptr::null_mut(), 77, combo.mods, combo.vk) }).join().unwrap();
        assert_eq!(taken, 0);
        assert_eq!(apply(vec![(Action::Search, None)]), vec![(Action::Search, true)]);
    }
}
