//! Auto-Type: digita na janela-alvo com SendInput em modo Unicode (KEYEVENTF_UNICODE), que não
//! depende do layout do teclado: acentos do ABNT2 saem certos.
//! Auto-Type: types into the target window with SendInput in Unicode mode (KEYEVENTF_UNICODE),
//! which does not depend on the keyboard layout: ABNT2 accents come out right.
//!
//! Proteções / Protections:
//! - espera o usuário soltar Ctrl/Shift/Alt/Win (senão as letras virariam atalhos);
//!   waits for the user to release Ctrl/Shift/Alt/Win (otherwise letters would become shortcuts);
//! - confere a janela (HWND + PID) ANTES DE CADA BLOCO e cancela se mudou;
//!   checks the window (HWND + PID) BEFORE EACH BLOCK and cancels if it changed;
//! - se o Windows bloquear (janela de administrador, UIPI), para e avisa;
//!   if Windows blocks it (administrator window, UIPI), stops and reports;
//! - apaga da memória os eventos de tecla montados. / wipes the built key events from memory.

use std::time::{Duration, Instant};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_TAB,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
use zeroize::Zeroizing;

use super::target::Target;

/// Um bloco de digitação. / One typing block.
pub enum Part {
    Text(Zeroizing<String>),
    Tab,
    /// Ctrl+A: o texto seguinte substitui o que já estava no campo. / Ctrl+A: the next text replaces what was in the field.
    SelectAll,
    /// Põe o foco num campo do programa (pela UI Automation); `false` = não deu.
    /// Focuses a field of the program (through UI Automation); `false` = it failed.
    Focus(Box<dyn Fn() -> bool + Send + Sync>),
}

/// Por que a digitação não aconteceu (ou parou). / Why typing did not happen (or stopped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeError {
    /// O usuário continuou segurando Ctrl/Shift/Alt. / The user kept holding Ctrl/Shift/Alt.
    KeysHeld,
    /// A janela mudou (outra janela ganhou o foco). / The window changed (another window got focus).
    WindowChanged,
    /// O Windows recusou (janela de administrador). / Windows refused (administrator window).
    Blocked,
}

const MODIFIERS: [u16; 5] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN];

fn modifiers_down() -> bool {
    // SAFETY: consulta do estado de teclas. / Key state query.
    MODIFIERS.iter().any(|&vk| unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000 != 0)
}

fn key(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

/// Eventos de um bloco. / Events of one block.
fn events(part: &Part) -> Vec<INPUT> {
    match part {
        Part::Tab => vec![key(VK_TAB, 0, 0), key(VK_TAB, 0, KEYEVENTF_KEYUP)],
        Part::SelectAll => vec![key(VK_CONTROL, 0, 0), key(0x41, 0, 0), key(0x41, 0, KEYEVENTF_KEYUP), key(VK_CONTROL, 0, KEYEVENTF_KEYUP)],
        Part::Focus(_) => Vec::new(),
        Part::Text(text) => text
            .encode_utf16()
            .flat_map(|unit| [key(0, unit, KEYEVENTF_UNICODE), key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)])
            .collect(),
    }
}

fn wipe(inputs: &mut [INPUT]) {
    for i in inputs.iter_mut() {
        // SAFETY: todos os INPUT daqui são de teclado. / All INPUTs here are keyboard ones.
        i.Anonymous.ki.wScan = 0;
    }
}

/// Digita os blocos na janela-alvo. / Types the blocks into the target window.
pub fn type_into(target: &Target, parts: &[Part]) -> Result<(), TypeError> {
    // 1. Espera soltar as teclas do atalho. / Waits for the hotkey keys to be released.
    let deadline = Instant::now() + Duration::from_secs(4);
    while modifiers_down() {
        if Instant::now() > deadline {
            return Err(TypeError::KeysHeld);
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    // 2. Devolve o foco à janela-alvo e confere. / Gives focus back to the target window and checks.
    // SAFETY: HWND capturado no atalho; se não existir mais, a chamada só falha.
    // SAFETY: HWND captured at hotkey time; if it no longer exists, the call just fails.
    unsafe { SetForegroundWindow(target.hwnd as _) };
    std::thread::sleep(Duration::from_millis(120));

    // 3. Bloco a bloco, conferindo a janela antes de cada um. / Block by block, checking the window before each one.
    for part in parts {
        if !target.still_foreground() {
            return Err(TypeError::WindowChanged);
        }
        if let Part::Focus(focus) = part {
            if !focus() {
                return Err(TypeError::WindowChanged);
            }
            std::thread::sleep(Duration::from_millis(80));
            continue;
        }
        let mut inputs = events(part);
        // SAFETY: vetor de INPUT válido do tamanho informado. / Valid INPUT vector of the given size.
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
        let complete = sent as usize == inputs.len();
        wipe(&mut inputs);
        if !complete {
            return Err(TypeError::Blocked);
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_unicode_events_for_accents_and_tab() {
        let e = events(&Part::Text(Zeroizing::new("ação€".into())));
        // 5 unidades UTF-16 × (descer + subir). / 5 UTF-16 units × (down + up).
        assert_eq!(e.len(), 10);
        // SAFETY: eventos de teclado. / Keyboard events.
        let scans: Vec<u16> = e.iter().step_by(2).map(|i| unsafe { i.Anonymous.ki.wScan }).collect();
        assert_eq!(String::from_utf16(&scans).unwrap(), "ação€");
        assert!(e.iter().all(|i| unsafe { i.Anonymous.ki.dwFlags } & KEYEVENTF_UNICODE != 0));
        // Emoji: par substituto (2 unidades). / Emoji: surrogate pair (2 units).
        assert_eq!(events(&Part::Text(Zeroizing::new("🔑".into()))).len(), 4);
        let tab = events(&Part::Tab);
        assert_eq!(unsafe { tab[0].Anonymous.ki.wVk }, VK_TAB);
        let mut e = e;
        wipe(&mut e);
        assert!(e.iter().all(|i| unsafe { i.Anonymous.ki.wScan } == 0));
    }
}
