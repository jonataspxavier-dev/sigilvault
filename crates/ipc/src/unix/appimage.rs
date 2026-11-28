//! Modo AppImage. Um AppImage roda de uma pasta temporária que muda a cada execução e não é do
//! root, então a "instalação protegida" do `.deb` não serve. Em vez disso:
//! - o app copia o conector (`sigilvault-host`) para uma pasta fixa do usuário e anota onde está o
//!   AppImage; o navegador aponta para essa cópia;
//! - o app só aceita um conector cujo **conteúdo** é idêntico ao que veio dentro do AppImage;
//! - o conector só aceita um app que diz ter sido aberto a partir do AppImage anotado (`APPIMAGE`).
//!
//! É uma proteção mais fraca que a do `.deb`: um programa do mesmo usuário ainda pode mexer nos
//! arquivos do usuário. Ele não consegue, porém, trocar o conector por outro e ser aceito.
//! AppImage mode. An AppImage runs from a temporary folder that changes on every run and is not
//! root's, so the `.deb`'s "protected install" does not apply. Instead:
//! - the app copies the connector (`sigilvault-host`) to a fixed user folder and notes where the
//!   AppImage is; the browser points at that copy;
//! - the app only accepts a connector whose **content** is identical to what came inside the AppImage;
//! - the connector only accepts an app that says it was opened from the noted AppImage (`APPIMAGE`).
//!
//! It is weaker protection than the `.deb`'s: a program of the same user can still touch the
//! user's files. It cannot, however, swap the connector for another and be accepted.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const HOST_FILE: &str = "sigilvault-host";
const LAUNCHER_FILE: &str = "appimage-path";

/// Pasta fixa do usuário para o conector: `~/.local/lib/sigilvault`. / Fixed user folder for the connector.
pub fn lib_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_absolute()).map(|h| h.join(".local/lib/sigilvault"))
}

/// O AppImage em que este processo está rodando, se for o caso. / The AppImage this process runs from, if any.
pub fn running_appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute() && p.is_file())
}

/// A cópia do conector na pasta do usuário. / The connector copy in the user's folder.
pub fn installed_host() -> Option<PathBuf> {
    lib_dir().map(|d| d.join(HOST_FILE))
}

/// O AppImage que o app anotou. / The AppImage the app noted.
pub fn registered_appimage() -> Option<PathBuf> {
    registered_appimage_in(&lib_dir()?)
}

fn registered_appimage_in(dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(dir.join(LAUNCHER_FILE)).ok()?;
    let path = PathBuf::from(text.trim());
    (path.is_absolute() && path.is_file()).then_some(path)
}

/// SHA-256 do conteúdo de um arquivo. / SHA-256 of a file's content.
pub fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    use io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buf[..n]);
    }
}

/// Os dois arquivos têm o mesmo conteúdo? Qualquer erro conta como "não".
/// Do the two files have the same content? Any error counts as "no".
pub fn same_content(a: &Path, b: &Path) -> bool {
    matches!((sha256_file(a), sha256_file(b)), (Ok(a), Ok(b)) if a == b)
}

/// Copia o conector para `dir` (só se o conteúdo mudou) e anota o AppImage. Devolve o caminho da cópia.
/// Copies the connector to `dir` (only if the content changed) and notes the AppImage. Returns the copy's path.
pub fn install_host_in(dir: &Path, bundled_host: &Path, appimage: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    let target = dir.join(HOST_FILE);
    if !(target.is_file() && same_content(&target, bundled_host)) {
        // Grava num arquivo novo e troca: o navegador pode estar usando o antigo agora.
        // Writes a new file and swaps: the browser may be using the old one right now.
        let tmp = dir.join(format!("{HOST_FILE}.new"));
        let _ = fs::remove_file(&tmp);
        let mut out = fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(&tmp)?;
        io::copy(&mut fs::File::open(bundled_host)?, &mut out)?;
        out.flush()?;
        out.sync_all()?;
        drop(out);
        fs::rename(&tmp, &target)?;
    }
    let launcher = dir.join(LAUNCHER_FILE);
    let note = appimage.to_string_lossy();
    if fs::read_to_string(&launcher).map(|t| t.trim() != note).unwrap_or(true) {
        let mut out = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&launcher)?;
        out.write_all(note.as_bytes())?;
    }
    Ok(target)
}

/// Copia o conector para a pasta do usuário. / Copies the connector to the user's folder.
pub fn install_host(bundled_host: &Path, appimage: &Path) -> io::Result<PathBuf> {
    let dir = lib_dir().ok_or_else(|| io::Error::other("sem HOME / no HOME"))?;
    install_host_in(&dir, bundled_host, appimage)
}

/// Valor de uma variável de ambiente de outro processo do mesmo usuário (`/proc/<pid>/environ`).
/// Value of an environment variable of another process of the same user (`/proc/<pid>/environ`).
pub fn env_of(pid: u32, name: &str) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{name}=");
    raw.split(|b| *b == 0).find_map(|entry| {
        let text = std::str::from_utf8(entry).ok()?;
        text.strip_prefix(&prefix).map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sigilvault-test-appimage-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installs_the_host_once_and_notes_the_appimage() {
        let root = temp("install");
        let bundled = root.join("bundled-host");
        let appimage = root.join("SigilVault.AppImage");
        fs::write(&bundled, b"host v1").unwrap();
        fs::write(&appimage, b"image").unwrap();
        let dir = root.join("lib");

        let installed = install_host_in(&dir, &bundled, &appimage).unwrap();
        assert_eq!(fs::read(&installed).unwrap(), b"host v1");
        assert_eq!(fs::metadata(&installed).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(registered_appimage_in(&dir), Some(appimage.clone()));

        // Mesmo conteúdo: não regrava (o arquivo continua o mesmo). / Same content: does not rewrite.
        let before = fs::metadata(&installed).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        install_host_in(&dir, &bundled, &appimage).unwrap();
        assert_eq!(fs::metadata(&installed).unwrap().modified().unwrap(), before);

        // Conector novo dentro do AppImage: a cópia é trocada. / New connector inside the AppImage: the copy is replaced.
        fs::write(&bundled, b"host v2").unwrap();
        install_host_in(&dir, &bundled, &appimage).unwrap();
        assert_eq!(fs::read(&installed).unwrap(), b"host v2");
        assert!(same_content(&installed, &bundled));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_swapped_host_does_not_match_the_bundled_one() {
        let root = temp("swap");
        let (good, evil) = (root.join("good"), root.join("evil"));
        fs::write(&good, b"good").unwrap();
        fs::write(&evil, b"evil").unwrap();
        assert!(!same_content(&good, &evil));
        assert!(same_content(&good, &good));
        assert!(!same_content(&good, &root.join("missing")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_or_moved_appimage_is_not_registered() {
        let root = temp("moved");
        let dir = root.join("lib");
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(registered_appimage_in(&dir), None);
        fs::write(dir.join(LAUNCHER_FILE), "/nao/existe/App.AppImage").unwrap();
        assert_eq!(registered_appimage_in(&dir), None);
        fs::write(dir.join(LAUNCHER_FILE), "relativo/App.AppImage").unwrap();
        assert_eq!(registered_appimage_in(&dir), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_the_environment_of_another_process() {
        let mut child = std::process::Command::new("sleep").arg("5").env("APPIMAGE", "/x/Y.AppImage").spawn().unwrap();
        // Logo após o `spawn` o filho ainda mostra o ambiente do pai: espera o `exec`.
        // Right after `spawn` the child still shows the parent's environment: waits for `exec`.
        let mut value = None;
        for _ in 0..100 {
            value = env_of(child.id(), "APPIMAGE");
            if value.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let other = env_of(child.id(), "SIGILVAULT_NAO_EXISTE");
        let _ = child.kill();
        assert_eq!(value.as_deref(), Some("/x/Y.AppImage"));
        assert_eq!(other, None);
        assert_eq!(env_of(u32::MAX, "APPIMAGE"), None);
    }
}
