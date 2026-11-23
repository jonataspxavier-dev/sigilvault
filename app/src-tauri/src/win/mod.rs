//! Integrações com o Windows. Único lugar do app com `unsafe`; cada bloco explica por que é seguro.
//! Windows integrations. The only place in the app with `unsafe`; each block explains why it is sound.

pub mod clipboard;
pub mod dpapi;
pub mod fields;
pub mod hotkeys;
pub mod startup;
pub mod system;
pub mod target;
pub mod typer;
pub mod watcher;

/// Converte texto para UTF-16 terminado em zero (formato das APIs do Windows).
/// Converts text to zero-terminated UTF-16 (the Windows API format).
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
