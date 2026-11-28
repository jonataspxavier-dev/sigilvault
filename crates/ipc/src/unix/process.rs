//! Quem é o processo do outro lado: caminho do executável, via `/proc`.
//! Who the process on the other side is: executable path, via `/proc`.

use std::io;
use std::path::{Path, PathBuf};

/// Caminho do executável de um processo. / Executable path of a process.
pub fn image_path(pid: u32) -> io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
}

/// São o mesmo arquivo? Resolve links; qualquer erro conta como "não". No Linux, maiúsculas e
/// minúsculas são diferentes. / Are they the same file? Resolves links; any error counts as "no".
/// On Linux, upper and lower case differ.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Arquivo vizinho do executável atual (mesma pasta). / Sibling file of the current executable (same folder).
pub fn sibling(name: &str) -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent().map(|dir| dir.join(name)).ok_or_else(|| io::Error::other("executável sem pasta / executable without folder"))
}

/// O que o conector deve abrir quando o app não está rodando: o app ao lado dele (instalação
/// `.deb`/`.rpm`) ou, no modo AppImage, o AppImage anotado pelo app.
/// What the connector must start when the app is not running: the app next to it (`.deb`/`.rpm`
/// install) or, in AppImage mode, the AppImage noted by the app.
pub fn app_to_launch() -> io::Result<PathBuf> {
    let beside = sibling(crate::APP_EXE)?;
    if beside.is_file() {
        return Ok(beside);
    }
    crate::appimage::registered_appimage().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "app não encontrado / app not found"))
}

/// O processo `pid` é o nosso app? Instalação normal: é o arquivo ao lado do conector e a
/// instalação é protegida. AppImage: o executável se chama como o app e foi aberto a partir do
/// AppImage anotado. / Is process `pid` our app? Normal install: it is the file next to the
/// connector and the install is protected. AppImage: the executable is named like the app and was
/// opened from the noted AppImage.
pub fn is_our_app(me: &Path, pid: u32) -> bool {
    let Ok(peer) = image_path(pid) else { return false };
    if let Ok(beside) = sibling(crate::APP_EXE)
        && beside.is_file()
    {
        return same_file(&peer, &beside) && crate::signature::peer_allowed(me, &peer);
    }
    let Some(registered) = crate::appimage::registered_appimage() else { return false };
    peer.file_name().is_some_and(|n| n == crate::APP_EXE)
        && crate::appimage::env_of(pid, "APPIMAGE").is_some_and(|v| same_file(Path::new(&v), &registered))
}

/// O arquivo é o nosso conector? Instalação normal: o que está ao lado do app. AppImage: a cópia
/// na pasta do usuário. / Is the file our connector? Normal install: the one next to the app.
/// AppImage: the copy in the user's folder.
pub fn is_our_host(peer: &Path) -> bool {
    if sibling(crate::HOST_EXE).is_ok_and(|beside| same_file(peer, &beside)) {
        return true;
    }
    crate::appimage::running_appimage().is_some() && crate::appimage::installed_host().is_some_and(|copy| same_file(peer, &copy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_image_and_compares() {
        let me = std::env::current_exe().unwrap();
        let from_proc = image_path(std::process::id()).unwrap();
        assert!(same_file(&from_proc, &me));
        assert!(!same_file(&me, Path::new("/nao/existe")));
        assert!(!same_file(&me, Path::new("/bin/sh")));
    }
}
