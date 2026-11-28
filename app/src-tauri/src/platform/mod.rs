//! Integrações com o sistema operacional. Cada sistema tem a sua pasta, e o resto do app só
//! enxerga os módulos reexportados aqui (`clipboard`, `secret`, `fields`, `hotkeys`…), todos com
//! a mesma API.
//! Operating system integrations. Each system has its own folder, and the rest of the app only
//! sees the modules re-exported here (`clipboard`, `secret`, `fields`, `hotkeys`…), all with the
//! same API.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(unix)]
mod linux;
#[cfg(unix)]
pub use linux::*;
