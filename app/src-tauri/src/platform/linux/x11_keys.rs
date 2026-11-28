//! Atalhos globais no X11 (`XGrabKey`). No Wayland isso não existe (veja o portal em `hotkeys`).
//! No X11 dá para saber na hora se a combinação já é de outro programa: o servidor recusa.
//! Global hotkeys on X11 (`XGrabKey`). On Wayland this does not exist (see the portal in `hotkeys`).
//! On X11 we know right away whether the combination already belongs to another program: the server refuses.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};
use x11rb::rust_connection::RustConnection;

use super::hotkeys::{ALT, CTRL, Combo, SHIFT, SUPER};

/// Bits que não fazem parte da combinação (Caps Lock e Num Lock): o atalho precisa funcionar com eles ligados.
/// Bits that are not part of the combination (Caps Lock and Num Lock): the hotkey must work with them on.
const LOCKS: [u16; 4] = [0, 0x02, 0x10, 0x12];

/// Código de símbolo (keysym) da tecla do atalho. / Keysym of the hotkey's key.
fn keysym(key: &str) -> Option<u32> {
    match key {
        "Space" => Some(0x20),
        k if k.len() == 1 => {
            let c = k.chars().next()?;
            // Letras em minúsculas e dígitos têm o próprio código ASCII. / Lowercase letters and digits use their ASCII code.
            c.is_ascii_alphanumeric().then(|| u32::from(c.to_ascii_lowercase() as u8))
        }
        k if k.starts_with('F') => k[1..].parse::<u32>().ok().filter(|n| (1..=24).contains(n)).map(|n| 0xFFBD + n),
        _ => None,
    }
}

fn modifiers(c: &Combo) -> u16 {
    let mut m = 0u16;
    if c.mods() & CTRL != 0 {
        m |= u16::from(ModMask::CONTROL);
    }
    if c.mods() & ALT != 0 {
        m |= u16::from(ModMask::M1);
    }
    if c.mods() & SHIFT != 0 {
        m |= u16::from(ModMask::SHIFT);
    }
    if c.mods() & SUPER != 0 {
        m |= u16::from(ModMask::M4);
    }
    m
}

/// Conexão ao servidor X e a janela raiz. / Connection to the X server and the root window.
pub struct Display {
    conn: RustConnection,
    root: u32,
    min_keycode: u8,
    max_keycode: u8,
}

impl Display {
    /// Conecta ao `DISPLAY`. / Connects to `DISPLAY`.
    pub fn open() -> Option<Self> {
        let (conn, screen) = RustConnection::connect(None).ok()?;
        let setup = conn.setup();
        let (root, min_keycode, max_keycode) = (setup.roots.get(screen)?.root, setup.min_keycode, setup.max_keycode);
        Some(Self { conn, root, min_keycode, max_keycode })
    }

    fn keycode(&self, keysym: u32) -> Option<u8> {
        let count = self.max_keycode - self.min_keycode + 1;
        let map = self.conn.get_keyboard_mapping(self.min_keycode, count).ok()?.reply().ok()?;
        let per = usize::from(map.keysyms_per_keycode);
        map.keysyms.chunks(per).position(|syms| syms.contains(&keysym)).map(|i| self.min_keycode + i as u8)
    }

    /// Pede a tecla ao servidor. `false` = outro programa já tem essa combinação (ou a tecla não existe).
    /// Asks the server for the key. `false` = another program already has that combination (or the key does not exist).
    pub fn grab(&self, combo: &Combo) -> bool {
        let Some(code) = keysym(combo.key()).and_then(|k| self.keycode(k)) else { return false };
        let base = modifiers(combo);
        let mut cookies = Vec::new();
        for lock in LOCKS {
            let mods = ModMask::from(base | lock);
            match self.conn.grab_key(true, self.root, mods, code, GrabMode::ASYNC, GrabMode::ASYNC) {
                Ok(cookie) => cookies.push(cookie),
                Err(_) => return false,
            }
        }
        // Qualquer recusa do servidor (BadAccess) quer dizer "já em uso". / Any server refusal (BadAccess) means "already in use".
        let ok = cookies.into_iter().all(|c| c.check().is_ok());
        if !ok {
            self.ungrab(combo);
        }
        ok
    }

    pub fn ungrab(&self, combo: &Combo) {
        let Some(code) = keysym(combo.key()).and_then(|k| self.keycode(k)) else { return };
        let base = modifiers(combo);
        for lock in LOCKS {
            let _ = self.conn.ungrab_key(code, self.root, ModMask::from(base | lock));
        }
        let _ = self.conn.flush();
    }

    /// Espera o próximo atalho apertado: devolve o código da tecla e os modificadores (sem Caps/Num Lock).
    /// Waits for the next hotkey press: returns the key code and the modifiers (without Caps/Num Lock).
    pub fn wait_press(&self) -> Option<(u8, u16)> {
        loop {
            match self.conn.wait_for_event().ok()? {
                x11rb::protocol::Event::KeyPress(e) => {
                    // Tira Caps Lock (0x02) e Num Lock (0x10) da conta. / Takes Caps Lock (0x02) and Num Lock (0x10) out.
                    let mods = u16::from(e.state) & 0x00FF & !0x0012;
                    return Some((e.detail, mods));
                }
                _ => continue,
            }
        }
    }

    /// O código de tecla de um atalho, para reconhecê-lo ao ser apertado. / A hotkey's key code, to recognize it when pressed.
    pub fn code_of(&self, combo: &Combo) -> Option<u8> {
        keysym(combo.key()).and_then(|k| self.keycode(k))
    }

    pub fn mods_of(&self, combo: &Combo) -> u16 {
        modifiers(combo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usa o servidor X de verdade: rode com `cargo test -- --ignored`. / Uses the real X server: run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "usa o servidor X / uses the X server"]
    fn a_taken_combination_is_refused() {
        let combo = super::super::hotkeys::parse("Ctrl+Alt+Shift+F12").unwrap();
        let first = Display::open().expect("sem X11 / no X11");
        let second = Display::open().unwrap();
        assert!(first.grab(&combo));
        assert!(!second.grab(&combo), "o servidor deveria recusar / the server should refuse");
        first.ungrab(&combo);
        assert!(second.grab(&combo));
        second.ungrab(&combo);
    }

    #[test]
    fn maps_keys_to_keysyms() {
        assert_eq!(keysym("Space"), Some(0x20));
        assert_eq!(keysym("K"), Some(u32::from(b'k')));
        assert_eq!(keysym("7"), Some(u32::from(b'7')));
        assert_eq!(keysym("F1"), Some(0xFFBE));
        assert_eq!(keysym("F12"), Some(0xFFC9));
        assert_eq!(keysym("F25"), None);
        assert_eq!(keysym("Enter"), None);
    }
}
