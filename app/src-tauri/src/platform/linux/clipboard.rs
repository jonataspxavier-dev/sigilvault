//! Área de transferência. Ainda não implementada no Linux: copiar devolve erro, e o app mostra
//! a mensagem de sempre ("não consegui copiar").
//! Clipboard. Not implemented on Linux yet: copying returns an error, and the app shows the usual
//! message ("could not copy").

/// Marca de uma cópia, para só limpar se ainda for a nossa senha.
/// Copy mark, to only clear if it is still our password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied;

/// Copia um segredo. / Copies a secret.
pub fn set_secret_text(_text: &str) -> Result<Copied, ()> {
    Err(())
}

/// Apaga a área de transferência se ela ainda tiver a nossa senha. / Clears the clipboard if it still holds our password.
pub fn clear_if_ours(_copied: &Copied) -> bool {
    false
}
