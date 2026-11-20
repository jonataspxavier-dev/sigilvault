//! Comunicação local entre o app da bandeja e o `sigilvault-host.exe` (o processo que o
//! navegador abre para o Native Messaging).
//! Local communication between the tray app and `sigilvault-host.exe` (the process the browser
//! starts for Native Messaging).
//!
//! - [`frame`]: mensagens com 4 bytes de tamanho + conteúdo (mesmo formato do Native Messaging).
//!   Messages with a 4-byte length + content (same format as Native Messaging).
//! - [`pipe`]: named pipe só do usuário atual, que não pode ser "sequestrado".
//!   Named pipe for the current user only, which cannot be "hijacked".
//! - [`process`] e [`signature`]: quem está do outro lado (caminho do executável e Authenticode).
//!   Who is on the other side (executable path and Authenticode).

pub mod frame;

#[cfg(windows)]
pub mod pipe;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod signature;
#[cfg(windows)]
mod user;

/// Nome do executável do host. / Host executable name.
pub const HOST_EXE: &str = "sigilvault-host.exe";
/// Nome do executável do app. / App executable name.
pub const APP_EXE: &str = "sigilvault-app.exe";
/// Nome do host registrado nos navegadores. / Host name registered in the browsers.
pub const HOST_NAME: &str = "app.sigilvault.host";

/// Maior mensagem entre extensão e app (64 KiB): bem acima do necessário, e corta abusos.
/// Largest message between extension and app (64 KiB): well above what is needed, and cuts abuse.
pub const MAX_MESSAGE: usize = 64 * 1024;
