//! Registro do host do Native Messaging nos navegadores do usuário. No Windows isso é feito pelo
//! instalador (chaves de registro); no Linux, são arquivos `.json` em pastas dos navegadores.
//! Registering the Native Messaging host in the user's browsers. On Windows the installer does it
//! (registry keys); on Linux, they are `.json` files in browser folders.
//!
//! Os pacotes .deb e .rpm já instalam os manifestos de sistema para Chrome, Chromium, Edge e
//! Firefox. O app ainda grava os do usuário nos navegadores que encontra instalados (Brave,
//! Vivaldi, Opera…), e só em pastas que o navegador já criou: quem não tem o navegador não ganha pasta.
//! The .deb and .rpm packages already install the system manifests for Chrome, Chromium, Edge and
//! Firefox. The app also writes the user's own in the browsers it finds installed (Brave, Vivaldi,
//! Opera…), and only in folders the browser already created: no browser, no folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sigilvault_ipc::{CHROMIUM_EXTENSION_ID, FIREFOX_EXTENSION_ID, HOST_NAME};

/// Famílias de navegadores, que usam formatos de manifesto diferentes.
/// Browser families, which use different manifest formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Chromium,
    Firefox,
}

/// Um navegador: pasta que prova que ele está instalado e pasta dos manifestos.
/// A browser: the folder that proves it is installed and the manifests folder.
struct Browser {
    family: Family,
    config: PathBuf,
    manifests: PathBuf,
}

fn browsers(home: &Path, config_home: &Path) -> Vec<Browser> {
    let chromium = |dir: &str| Browser { family: Family::Chromium, config: config_home.join(dir), manifests: config_home.join(dir).join("NativeMessagingHosts") };
    vec![
        chromium("google-chrome"),
        chromium("chromium"),
        chromium("BraveSoftware/Brave-Browser"),
        chromium("microsoft-edge"),
        chromium("vivaldi"),
        chromium("opera"),
        Browser { family: Family::Firefox, config: home.join(".mozilla"), manifests: home.join(".mozilla/native-messaging-hosts") },
    ]
}

/// Texto do manifesto de uma família, apontando para `host` (caminho absoluto, como o Linux exige).
/// Manifest text for a family, pointing at `host` (absolute path, as Linux requires).
fn manifest(family: Family, host: &Path) -> String {
    let mut value = serde_json::json!({
        "name": HOST_NAME,
        "description": "SigilVault",
        "path": host.to_string_lossy(),
        "type": "stdio",
    });
    match family {
        Family::Chromium => value["allowed_origins"] = serde_json::json!([format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")]),
        Family::Firefox => value["allowed_extensions"] = serde_json::json!([FIREFOX_EXTENSION_ID]),
    }
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

fn install_in(browsers: &[Browser], host: &Path) -> Vec<PathBuf> {
    let mut written = Vec::new();
    for b in browsers {
        if !b.config.is_dir() {
            continue;
        }
        let file = b.manifests.join(format!("{HOST_NAME}.json"));
        let text = manifest(b.family, host);
        // Já está certo: não mexe. / Already right: leave it.
        if fs::read_to_string(&file).is_ok_and(|current| current == text) {
            continue;
        }
        if fs::create_dir_all(&b.manifests).and_then(|()| fs::write(&file, text)).is_ok() {
            written.push(file);
        }
    }
    written
}

/// Grava os manifestos nos navegadores instalados. Devolve os arquivos que mudaram.
/// Writes the manifests in the installed browsers. Returns the files that changed.
pub fn install_user_manifests(host: &Path) -> io::Result<Vec<PathBuf>> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| io::Error::other("sem HOME / no HOME"))?;
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(".config"));
    Ok(install_in(&browsers(&home, &config_home), host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_have_the_right_shape_per_family() {
        let host = Path::new("/usr/bin/sigilvault-host");
        let chromium: serde_json::Value = serde_json::from_str(&manifest(Family::Chromium, host)).unwrap();
        assert_eq!(chromium["name"], "app.sigilvault.host");
        assert_eq!(chromium["path"], "/usr/bin/sigilvault-host");
        assert_eq!(chromium["type"], "stdio");
        assert_eq!(chromium["allowed_origins"][0], format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/"));
        assert!(chromium.get("allowed_extensions").is_none());
        let firefox: serde_json::Value = serde_json::from_str(&manifest(Family::Firefox, host)).unwrap();
        assert_eq!(firefox["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
        assert!(firefox.get("allowed_origins").is_none());
    }

    #[test]
    fn only_installed_browsers_get_a_manifest_and_nothing_is_rewritten() {
        let dir = std::env::temp_dir().join(format!("sigilvault-test-nm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let config = dir.join(".config");
        fs::create_dir_all(config.join("chromium")).unwrap();
        fs::create_dir_all(dir.join(".mozilla")).unwrap();
        let host = Path::new("/usr/bin/sigilvault-host");
        let list = browsers(&dir, &config);

        let written = install_in(&list, host);
        assert_eq!(written.len(), 2);
        assert!(config.join("chromium/NativeMessagingHosts/app.sigilvault.host.json").is_file());
        assert!(dir.join(".mozilla/native-messaging-hosts/app.sigilvault.host.json").is_file());
        // Brave não está instalado: nada foi criado para ele. / Brave is not installed: nothing was created for it.
        assert!(!config.join("BraveSoftware").exists());
        assert!(!config.join("google-chrome").exists());

        // Segunda vez: já está certo. Mudou o host: regrava. / Second time: already right. Host changed: rewrites.
        assert!(install_in(&list, host).is_empty());
        assert_eq!(install_in(&list, Path::new("/opt/outro/sigilvault-host")).len(), 2);
        let _ = fs::remove_dir_all(dir);
    }
}
