//! Proteção contra "voltar versão": guarda, fora do cofre e protegida pela DPAPI, a maior revisão
//! já vista de cada cofre. Se alguém trocar o arquivo por uma cópia antiga, o app avisa.
//! Rollback protection: stores, outside the vault and protected by DPAPI, the highest revision
//! ever seen for each vault. If someone swaps the file for an old copy, the app warns.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::storage::atomic_write;

/// Funções que protegem e desprotegem o arquivo (DPAPI no Windows; identidade nos testes).
/// Functions that protect and unprotect the file (DPAPI on Windows; identity in tests).
#[derive(Clone, Copy)]
pub struct Codec {
    pub protect: fn(&[u8]) -> Option<Vec<u8>>,
    pub unprotect: fn(&[u8]) -> Option<Vec<u8>>,
}

impl Codec {
    /// DPAPI da conta atual. / Current account DPAPI.
    pub const DPAPI: Self = Self { protect: crate::win::dpapi::protect, unprotect: crate::win::dpapi::unprotect };
}

/// Resultado da verificação. / Check result.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    /// Revisão igual ou mais nova que a esperada. / Revision equal to or newer than expected.
    Ok,
    /// O arquivo é mais antigo do que o último visto. / The file is older than the last one seen.
    Older { expected: u64, found: u64 },
}

/// Guardião das revisões. / Revision guard.
pub struct RevisionGuard {
    path: PathBuf,
    codec: Codec,
    seen: BTreeMap<String, u64>,
}

impl RevisionGuard {
    /// Carrega o arquivo protegido. Se não existir ou não abrir, começa vazio.
    /// Loads the protected file. If missing or unreadable, starts empty.
    pub fn load(path: &Path, codec: Codec) -> Self {
        let seen = std::fs::read(path)
            .ok()
            .and_then(|b| (codec.unprotect)(&b))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path: path.to_owned(), codec, seen }
    }

    /// Compara a revisão do arquivo aberto com a maior já vista.
    /// Compares the opened file's revision with the highest seen.
    pub fn check(&self, vault_id: &str, revision: u64) -> Check {
        match self.seen.get(vault_id) {
            Some(&expected) if revision < expected => Check::Older { expected, found: revision },
            _ => Check::Ok,
        }
    }

    /// Registra a revisão (só aumenta) e grava. / Records the revision (only grows) and saves.
    pub fn record(&mut self, vault_id: &str, revision: u64) {
        let entry = self.seen.entry(vault_id.to_owned()).or_insert(0);
        if revision <= *entry && self.path.exists() {
            return;
        }
        *entry = (*entry).max(revision);
        if let Some(bytes) = serde_json::to_vec(&self.seen).ok().and_then(|j| (self.codec.protect)(&j)) {
            let _ = atomic_write(&self.path, &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Codec = Codec { protect: |b| Some(b.to_vec()), unprotect: |b| Some(b.to_vec()) };

    #[test]
    fn detects_older_file_and_persists() {
        let dir = crate::storage::test_dir("rollback");
        let path = dir.join("revision.guard");
        let mut g = RevisionGuard::load(&path, PLAIN);
        assert_eq!(g.check("abc", 1), Check::Ok);
        g.record("abc", 5);
        g.record("abc", 3); // nunca diminui / never decreases
        let g = RevisionGuard::load(&path, PLAIN);
        assert_eq!(g.check("abc", 5), Check::Ok);
        assert_eq!(g.check("abc", 6), Check::Ok);
        assert_eq!(g.check("abc", 4), Check::Older { expected: 5, found: 4 });
        assert_eq!(g.check("outro", 1), Check::Ok);
    }

    #[test]
    fn works_with_dpapi() {
        let dir = crate::storage::test_dir("rollback-dpapi");
        let path = dir.join("revision.guard");
        RevisionGuard::load(&path, Codec::DPAPI).record("abc", 7);
        // O arquivo em disco não é o JSON em texto puro. / The file on disk is not plain JSON.
        assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("abc"));
        assert_eq!(RevisionGuard::load(&path, Codec::DPAPI).check("abc", 6), Check::Older { expected: 7, found: 6 });
    }
}
