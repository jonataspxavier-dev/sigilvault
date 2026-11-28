//! "Instalação protegida": o executável e todas as pastas acima dele pertencem ao root e ninguém
//! mais pode alterá-los. É o que impede um programa comum de trocar o host ou o app.
//! "Protected install": the executable and every folder above it belong to root and nobody else
//! can change them. This is what stops an ordinary program from swapping the host or the app.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Do root e sem escrita para grupo ou outros? / Owned by root and not writable by group or others?
fn root_only(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
}

/// A pasta e todas as pastas acima são do root e protegidas? / Is the folder and every folder above it root-owned and protected?
pub fn folder_protected(dir: &Path) -> bool {
    let Ok(dir) = dir.canonicalize() else { return false };
    dir.ancestors().all(root_only)
}

/// O arquivo é do root e protegido? / Is the file root-owned and protected?
pub fn file_protected(file: &Path) -> bool {
    file.canonicalize().is_ok_and(|f| root_only(&f))
}

/// Os dois arquivos estão na mesma pasta protegida? / Are both files in the same protected folder?
pub fn protected_install(own: &Path, peer: &Path) -> bool {
    let (Ok(own), Ok(peer)) = (own.canonicalize(), peer.canonicalize()) else { return false };
    let (Some(a), Some(b)) = (own.parent(), peer.parent()) else { return false };
    a == b && folder_protected(a) && file_protected(&own) && file_protected(&peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_folder_is_not_protected() {
        assert!(!folder_protected(&std::env::temp_dir()));
    }

    #[test]
    fn system_binaries_are_protected_for_a_normal_user() {
        // Se o teste roda como root dentro de um contêiner, o resultado é o mesmo, então vale.
        // If the test runs as root in a container, the result is the same, so it holds.
        assert!(folder_protected(Path::new("/usr/bin")));
        assert!(file_protected(Path::new("/bin/sh")));
        assert!(protected_install(Path::new("/bin/sh"), Path::new("/bin/sh")));
        // Pastas diferentes não valem como "lado a lado". / Different folders do not count as "side by side".
        assert!(!protected_install(Path::new("/bin/sh"), &std::env::current_exe().unwrap()));
    }

    #[test]
    fn missing_path_counts_as_unprotected() {
        assert!(!folder_protected(Path::new("/nao/existe")));
        assert!(!file_protected(Path::new("/nao/existe")));
    }
}
