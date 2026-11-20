//! Pareamento entre a extensão e o app (OPAQUE) e o canal criptografado de cada conexão.
//! O mesmo código roda no app (lado servidor) e, compilado para WebAssembly, na extensão (lado
//! cliente), para que os dois lados nunca divirjam.
//!
//! Pairing between the extension and the app (OPAQUE) and the encrypted channel of each
//! connection. The same code runs in the app (server side) and, compiled to WebAssembly, in the
//! extension (client side), so both sides never diverge.
//!
//! Fluxo / Flow:
//! 1. O app sorteia um [`PairingCode`] curto e o mostra na tela. O usuário digita na extensão.
//!    The app draws a short [`PairingCode`] and shows it. The user types it in the extension.
//! 2. OPAQUE com o código como senha: só quem viu a tela consegue terminar, e uma escuta não
//!    permite testar códigos offline. / OPAQUE with the code as password: only someone who saw
//!    the screen can finish, and eavesdropping does not allow testing codes offline.
//! 3. Da chave de sessão sai a [`pairing_key`] permanente. / The permanent [`pairing_key`] comes
//!    from the session key.
//! 4. Cada conexão depois disso usa [`Channel`], com chaves novas. / Each later connection uses
//!    [`Channel`], with fresh keys.

pub mod channel;
pub mod code;
pub mod pake;
mod rng;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use channel::{Channel, Role};
pub use code::PairingCode;
pub use pake::{ClientPairing, ServerLogin, ServerPairing, SessionKey, pairing_key};
pub use rng::FillRng;

/// Erros do pareamento e do canal. Não dizem qual etapa falhou, de propósito.
/// Pairing and channel errors. They do not say which step failed, on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Código errado, mensagem adulterada ou fora de ordem. / Wrong code, tampered or out-of-order message.
    Rejected,
    /// Código com formato inválido. / Malformed code.
    MalformedCode,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Error::Rejected => "pareamento recusado / pairing rejected",
            Error::MalformedCode => "código malformado / malformed code",
        })
    }
}

impl std::error::Error for Error {}
