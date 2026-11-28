//! Proteção de dados presa à conta do usuário: o que o DPAPI faz no Windows.
//! Data protection tied to the user account: what DPAPI does on Windows.
//!
//! Funcionamento / How it works:
//! - Existe uma chave de 256 bits por dispositivo. Ela fica no chaveiro do sistema (Secret
//!   Service: GNOME Keyring, KWallet…), que só abre com o login do usuário.
//!   There is one 256-bit key per device. It lives in the system keyring (Secret Service: GNOME
//!   Keyring, KWallet…), which only opens with the user's login.
//! - Sem chaveiro (sessão sem D-Bus, por exemplo), a chave fica num arquivo `0600` na pasta de
//!   dados. É uma proteção mais fraca, e [`backend`] avisa isso para a interface.
//!   Without a keyring (a session without D-Bus, for instance), the key lives in a `0600` file in
//!   the data folder. That is weaker protection, and [`backend`] tells the UI.
//! - Os dados são cifrados com XChaCha20-Poly1305; o rótulo entra como dado autenticado, então
//!   um blob de um uso não abre como se fosse de outro.
//!   Data is encrypted with XChaCha20-Poly1305; the label is authenticated data, so a blob from
//!   one use does not open as if it were from another.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sigilvault_core::crypto::{self, KEY_LEN, NONCE_LEN, Sealed};
use zeroize::Zeroizing;

/// Entropia extra do uso sem rótulo (o contador de versão), igual à do Windows.
/// Extra entropy of the unlabeled use (the version counter), same as on Windows.
const DEFAULT_LABEL: &[u8] = b"SigilVault revision guard v1";
/// Versão do formato dos blobs. / Blob format version.
const VERSION: u8 = 1;
const FILE_NAME: &str = "device.key";

/// Onde está a chave do dispositivo. / Where the device key lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// No chaveiro do sistema. / In the system keyring.
    Keyring,
    /// Num arquivo na pasta de dados (mais fraco). / In a file in the data folder (weaker).
    File,
    /// Não foi possível ter uma chave. / No key could be had.
    None,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static KEY: OnceLock<(Backend, Option<Zeroizing<[u8; KEY_LEN]>>)> = OnceLock::new();

/// Diz onde fica a pasta de dados (para o arquivo de reserva). Chamar antes do primeiro uso.
/// Tells where the data folder is (for the fallback file). Call before first use.
pub fn configure(dir: &Path) {
    let _ = DATA_DIR.set(dir.to_path_buf());
}

fn data_dir() -> PathBuf {
    if let Some(dir) = DATA_DIR.get() {
        return dir.clone();
    }
    if cfg!(test) {
        // Os testes nunca encostam na pasta de verdade. / Tests never touch the real folder.
        // SAFETY: `geteuid` não recebe argumentos e não falha. / `geteuid` takes no arguments and cannot fail.
        return std::env::temp_dir().join(format!("sigilvault-test-device-{}", unsafe { libc::geteuid() }));
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("app.sigilvault.desktop")
}

/// Qual proteção está valendo. Abre (e, se preciso, cria) a chave na primeira chamada.
/// Which protection is in effect. Opens (and, if needed, creates) the key on the first call.
pub fn backend() -> Backend {
    key_slot().0
}

fn key_slot() -> &'static (Backend, Option<Zeroizing<[u8; KEY_LEN]>>) {
    KEY.get_or_init(|| {
        // Nos testes não se fala com o chaveiro real. / Tests do not talk to the real keyring.
        if !cfg!(test)
            && let Some(key) = keyring_key()
        {
            return (Backend::Keyring, Some(key));
        }
        match file_key(&data_dir()) {
            Ok(key) => (Backend::File, Some(key)),
            Err(_) => (Backend::None, None),
        }
    })
}

fn decode_key(hex: &str) -> Option<Zeroizing<[u8; KEY_LEN]>> {
    let bytes = Zeroizing::new(sigilvault_core::hex::decode(hex.trim())?);
    let key: [u8; KEY_LEN] = bytes.as_slice().try_into().ok()?;
    Some(Zeroizing::new(key))
}

/// Chave do chaveiro do sistema; cria uma se ainda não houver. Roda numa thread própria porque
/// precisa de um runtime assíncrono, e o app pode já estar dentro de outro.
/// Key from the system keyring; creates one if there is none yet. Runs on its own thread because
/// it needs an async runtime, and the app may already be inside another.
fn keyring_key() -> Option<Zeroizing<[u8; KEY_LEN]>> {
    std::thread::Builder::new()
        .name("sigilvault-keyring".into())
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
            runtime.block_on(async {
                let attrs = &[("application", "sigilvault"), ("purpose", "device-key")];
                let keyring = oo7::Keyring::new().await.ok()?;
                keyring.unlock().await.ok()?;
                if let Some(item) = keyring.search_items(attrs).await.ok()?.into_iter().next() {
                    let secret = item.secret().await.ok()?;
                    return decode_key(std::str::from_utf8(secret.as_bytes()).ok()?);
                }
                let key = crypto::random_key().ok()?;
                let hex = Zeroizing::new(sigilvault_core::hex::encode(&*key));
                keyring.create_item("SigilVault", attrs, oo7::Secret::text(hex.as_str()), true).await.ok()?;
                Some(key)
            })
        })
        .ok()?
        .join()
        .ok()
        .flatten()
}

/// Chave do arquivo `device.key` (modo `0600`); cria se não existir.
/// Key from the `device.key` file (mode `0600`); creates it if missing.
fn file_key(dir: &Path) -> io::Result<Zeroizing<[u8; KEY_LEN]>> {
    let path = dir.join(FILE_NAME);
    for _ in 0..2 {
        if let Ok(text) = fs::read_to_string(&path) {
            let text = Zeroizing::new(text);
            let meta = fs::metadata(&path)?;
            // Arquivo legível por outros não serve de segredo. / A file readable by others is no secret.
            if meta.permissions().mode() & 0o077 != 0 {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            }
            return decode_key(&text).ok_or_else(|| io::Error::other("chave ilegível / unreadable key"));
        }
        fs::create_dir_all(dir)?;
        let key = crypto::random_key().map_err(|_| io::Error::other("sem acaso / no randomness"))?;
        let hex = Zeroizing::new(sigilvault_core::hex::encode(&*key));
        match fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path) {
            Ok(mut f) => {
                f.write_all(hex.as_bytes())?;
                f.sync_all()?;
                return Ok(key);
            }
            // Outro processo criou ao mesmo tempo: lê a dele. / Another process created it meanwhile: read theirs.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("não consegui obter a chave / could not get the key"))
}

fn device_key() -> Option<&'static [u8; KEY_LEN]> {
    key_slot().1.as_deref()
}

/// Protege os dados (contador de versão). / Protects the data (version counter).
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    protect_labeled(data, DEFAULT_LABEL)
}

/// Abre dados protegidos (contador de versão). / Opens protected data (version counter).
pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    unprotect_labeled(data, DEFAULT_LABEL)
}

/// Protege com um rótulo, que também precisa estar certo para abrir. / Protects with a label that must also match to open.
pub fn protect_labeled(data: &[u8], label: &[u8]) -> Option<Vec<u8>> {
    let sealed = crypto::seal(device_key()?, data, label).ok()?;
    let mut out = Vec::with_capacity(1 + NONCE_LEN + sealed.ciphertext.len());
    out.push(VERSION);
    out.extend_from_slice(&sealed.nonce);
    out.extend_from_slice(&sealed.ciphertext);
    Some(out)
}

/// Abre dados protegidos com rótulo. Qualquer problema (chave de outra conta, dado alterado,
/// rótulo errado) devolve `None`. / Opens label-protected data. Any problem (another account's key,
/// altered data, wrong label) returns `None`.
pub fn unprotect_labeled(data: &[u8], label: &[u8]) -> Option<Vec<u8>> {
    let (&version, rest) = data.split_first()?;
    if version != VERSION || rest.len() < NONCE_LEN {
        return None;
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let sealed = Sealed { nonce: nonce.try_into().ok()?, ciphertext: ciphertext.to_vec() };
    crypto::open(device_key()?, &sealed.nonce, &sealed.ciphertext, label).ok().map(|plain| plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_label_binding() {
        let blob = protect_labeled(b"segredo", b"rotulo A").unwrap();
        assert_ne!(&blob[..], b"segredo");
        assert_eq!(unprotect_labeled(&blob, b"rotulo A").unwrap(), b"segredo");
        // Rótulo errado, dado alterado, versão desconhecida e lixo: nada abre.
        // Wrong label, altered data, unknown version and junk: nothing opens.
        assert!(unprotect_labeled(&blob, b"rotulo B").is_none());
        let mut flipped = blob.clone();
        *flipped.last_mut().unwrap() ^= 1;
        assert!(unprotect_labeled(&flipped, b"rotulo A").is_none());
        let mut v2 = blob.clone();
        v2[0] = 2;
        assert!(unprotect_labeled(&v2, b"rotulo A").is_none());
        assert!(unprotect_labeled(b"", b"rotulo A").is_none());
        assert!(unprotect_labeled(&[1, 2, 3], b"rotulo A").is_none());
    }

    #[test]
    fn default_use_roundtrips_and_is_random_each_time() {
        let a = protect(b"{\"v\":7}").unwrap();
        let b = protect(b"{\"v\":7}").unwrap();
        assert_ne!(a, b);
        assert_eq!(unprotect(&a).unwrap(), b"{\"v\":7}");
    }

    #[test]
    fn file_key_is_created_private_and_stable() {
        let dir = std::env::temp_dir().join(format!("sigilvault-test-filekey-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let first = file_key(&dir).unwrap();
        assert_eq!(fs::metadata(dir.join(FILE_NAME)).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(*first, *file_key(&dir).unwrap());
        // Arquivo aberto demais é corrigido ao ler. / An over-open file is fixed on read.
        fs::set_permissions(dir.join(FILE_NAME), fs::Permissions::from_mode(0o644)).unwrap();
        file_key(&dir).unwrap();
        assert_eq!(fs::metadata(dir.join(FILE_NAME)).unwrap().permissions().mode() & 0o777, 0o600);
        // Conteúdo ruim é erro, não uma chave nova (senão os dados antigos ficariam perdidos sem aviso).
        // Bad content is an error, not a new key (otherwise old data would be lost silently).
        fs::write(dir.join(FILE_NAME), "lixo").unwrap();
        assert!(file_key(&dir).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
