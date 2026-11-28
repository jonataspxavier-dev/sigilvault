//! Área de transferência pelo GTK. A cópia leva a marca `x-kde-passwordManagerHint: secret`, que
//! o Klipper, o CopyQ e outros gerenciadores respeitam para não guardar a senha no histórico.
//! Clipboard through GTK. The copy carries the `x-kde-passwordManagerHint: secret` mark, which
//! Klipper, CopyQ and other managers honor to avoid keeping the password in history.
//!
//! O GTK só pode ser usado na thread principal, então tudo passa por [`on_main`]. No Wayland, o
//! sistema só deixa mexer na área de transferência a quem tem o foco: limpar com a janela do
//! SigilVault sem foco pode falhar (a senha então vale até ser trocada por outra cópia).
//! GTK may only be used on the main thread, so everything goes through [`on_main`]. On Wayland,
//! the system only lets the focused program touch the clipboard: clearing while SigilVault's
//! window is unfocused may fail (the password then stays until replaced by another copy).

use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::{Clipboard, TargetEntry, TargetFlags, gdk};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const HINT_TARGET: &str = "x-kde-passwordManagerHint";
/// Quanto esperar a thread principal. / How long to wait for the main thread.
const MAIN_WAIT: Duration = Duration::from_secs(3);

/// Marca de uma cópia: impressão digital (SHA-256 com sal) do texto. Nunca guardamos a senha em
/// si, só a impressão, para reconhecê-la na hora de limpar.
/// Mark of a copy: fingerprint (salted SHA-256) of the text. We never keep the password itself,
/// only the fingerprint, to recognize it when clearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    salt: [u8; 16],
    digest: [u8; 32],
}

fn fingerprint(salt: &[u8; 16], text: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"sigilvault clipboard v1");
    hasher.update(salt);
    hasher.update(text.as_bytes());
    hasher.finalize().into()
}

/// Roda `f` na thread principal (a do GTK) e espera o resultado. Se a thread principal já é a atual,
/// roda direto. / Runs `f` on the main (GTK) thread and waits for the result. If the main thread
/// is already the current one, runs directly.
fn on_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    glib::MainContext::default().invoke(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(MAIN_WAIT).ok()
}

fn clipboard() -> Clipboard {
    Clipboard::get(&gdk::SELECTION_CLIPBOARD)
}

/// Copia um segredo, com a marca que pede aos gerenciadores de histórico para ignorá-lo.
/// Copies a secret, with the mark asking history managers to ignore it.
pub fn set_secret_text(text: &str) -> Result<Copied, ()> {
    let salt: [u8; 16] = sigilvault_core::crypto::random_array().map_err(|_| ())?;
    let digest = fingerprint(&salt, text);
    let text = Zeroizing::new(text.to_owned());
    let done = on_main(move || {
        let targets = [
            TargetEntry::new("UTF8_STRING", TargetFlags::empty(), 0),
            TargetEntry::new("text/plain;charset=utf-8", TargetFlags::empty(), 0),
            TargetEntry::new("text/plain", TargetFlags::empty(), 0),
            TargetEntry::new("STRING", TargetFlags::empty(), 0),
            TargetEntry::new(HINT_TARGET, TargetFlags::empty(), 1),
        ];
        // O texto fica dentro da função de entrega e é apagado quando o GTK a descarta (outro
        // programa assumiu a área de transferência, ou nós a limpamos).
        // The text lives inside the delivery function and is wiped when GTK drops it (another
        // program took the clipboard, or we cleared it).
        clipboard().set_with_data(&targets, move |_, data, info| {
            if info == 1 {
                let hint = b"secret";
                // SAFETY: `data` é válido durante a chamada; o conteúdo é copiado pelo GTK.
                // SAFETY: `data` is valid during the call; GTK copies the content.
                unsafe {
                    gtk::ffi::gtk_selection_data_set(data.to_glib_none().0, data.target().to_glib_none().0, 8, hint.as_ptr(), hint.len() as i32);
                }
            } else {
                data.set_text(&text);
            }
        })
    });
    if done == Some(true) { Ok(Copied { salt, digest }) } else { Err(()) }
}

/// Esvazia a área de transferência se ela ainda tiver a nossa senha (mesma impressão digital).
/// Clears the clipboard if it still holds our password (same fingerprint).
pub fn clear_if_ours(copied: &Copied) -> bool {
    let copied = copied.clone();
    on_main(move || {
        let board = clipboard();
        let ours = board.wait_for_text().is_some_and(|text| fingerprint(&copied.salt, &text) == copied.digest);
        if ours {
            // Sobrescreve (mesmo que outro programa tenha regravado o texto) e solta a posse.
            // Overwrites (even if another program rewrote the text) and lets go of ownership.
            board.set_text("");
            board.clear();
        }
        ours
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_depends_on_salt_and_text() {
        let a = fingerprint(&[1; 16], "senha");
        assert_eq!(a, fingerprint(&[1; 16], "senha"));
        assert_ne!(a, fingerprint(&[2; 16], "senha"));
        assert_ne!(a, fingerprint(&[1; 16], "outra"));
    }
}
