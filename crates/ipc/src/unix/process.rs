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
