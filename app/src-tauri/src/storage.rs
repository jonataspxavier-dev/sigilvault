//! Caminhos dos arquivos do app e gravação atômica.
//! App file paths and atomic writes.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Arquivos na pasta de dados do usuário (AppData).
/// Files in the user's data folder (AppData).
#[derive(Debug, Clone)]
pub struct Paths {
    /// O cofre cifrado. / The encrypted vault.
    pub vault: PathBuf,
    /// Índice local do perfil (nome na tela de login). Nunca é exportado.
    /// Local profile index (name on the login screen). Never exported.
    pub profile: PathBuf,
    /// Preferências. / Preferences.
    pub settings: PathBuf,
    /// Contador de tentativas de desbloqueio. / Unlock attempt counter.
    pub attempts: PathBuf,
    /// Contador de versão protegido pela DPAPI. / DPAPI-protected version counter.
    pub revision_guard: PathBuf,
    /// Pasta dos backups automáticos. / Automatic backups folder.
    pub backups: PathBuf,
    /// Extensões pareadas (chaves protegidas pela DPAPI). / Paired extensions (DPAPI-protected keys).
    pub pairings: PathBuf,
    /// Registro dos preenchimentos (protegido pela DPAPI). / Fill log (DPAPI-protected).
    pub fill_log: PathBuf,
}

impl Paths {
    /// Monta os caminhos dentro de `dir`, criando a pasta se preciso.
    /// Builds the paths inside `dir`, creating the folder if needed.
    pub fn new(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            vault: dir.join("cofre.sigil"),
            profile: dir.join("profile.json"),
            settings: dir.join("settings.json"),
            attempts: dir.join("attempts.json"),
            revision_guard: dir.join("revision.guard"),
            backups: dir.join("backups"),
            pairings: dir.join("pairings.json"),
            fill_log: dir.join("fill-log.bin"),
        })
    }
}

/// SHA-256 de um conteúdo. / SHA-256 of some content.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).into()
}

/// Grava num `.tmp`, força a ida ao disco e substitui o original.
/// Writes a `.tmp`, flushes it to disk and replaces the original.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Pasta temporária única para testes. / Unique temporary folder for tests.
#[cfg(test)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sigilvault-app-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
