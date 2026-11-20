//! Backups locais rotativos: guarda os últimos [`KEEP`] arquivos `.sigilbak` na pasta do app.
//! O conteúdo é cifrado pelo núcleo; aqui só cuidamos dos arquivos.
//! Rotating local backups: keeps the last [`KEEP`] `.sigilbak` files in the app folder.
//! Content is encrypted by the core; here we only handle files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::atomic_write;

/// Quantos backups manter. / How many backups to keep.
pub const KEEP: usize = 5;
const PREFIX: &str = "backup-";
const EXT: &str = ".sigilbak";
static SEQ: AtomicU32 = AtomicU32::new(0);

/// O nome é um identificador válido de backup? (A interface só manda nomes, nunca caminhos.)
/// Is the name a valid backup identifier? (The UI only sends names, never paths.)
pub fn is_valid_name(name: &str) -> bool {
    name.starts_with(PREFIX)
        && name.ends_with(EXT)
        && name.len() < 80
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Backups existentes, do mais novo para o mais antigo. / Existing backups, newest first.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(is_valid_name))
        .collect();
    // O nome começa com a data em largura fixa, então ordem alfabética = ordem de data.
    // The name starts with a fixed-width date, so alphabetical order = date order.
    files.sort();
    files.reverse();
    files
}

/// Grava um backup novo e apaga os que passarem de [`KEEP`]. / Writes a new backup and deletes those beyond [`KEEP`].
pub fn write(dir: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) % 1000;
    let path = dir.join(format!("{PREFIX}{:012}-{:09}-{seq:03}{EXT}", now.as_secs(), now.subsec_nanos()));
    atomic_write(&path, bytes)?;
    prune(dir, KEEP);
    Ok(path)
}

/// Mantém só os `keep` mais novos. / Keeps only the `keep` newest.
pub fn prune(dir: &Path, keep: usize) {
    for old in list(dir).into_iter().skip(keep) {
        let _ = fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated() {
        assert!(is_valid_name("backup-000000000001-000000002-003.sigilbak"));
        for bad in ["../cofre.sigil", "backup-1.sigil", "backup-..\\x.sigilbak", "c:\\backup-1.sigilbak", "outro.sigilbak"] {
            assert!(!is_valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn keeps_only_the_newest() {
        let dir = crate::storage::test_dir("backups");
        for i in 0..8u8 {
            write(&dir, &[i]).unwrap();
        }
        let files = list(&dir);
        assert_eq!(files.len(), KEEP);
        // O mais novo primeiro. / Newest first.
        assert_eq!(fs::read(&files[0]).unwrap(), vec![7]);
        assert_eq!(fs::read(&files[KEEP - 1]).unwrap(), vec![3]);
    }
}
