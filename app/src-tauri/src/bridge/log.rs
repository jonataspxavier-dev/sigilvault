//! Registro dos preenchimentos feitos pela extensão (o planejamento pede que o app mostre).
//! Guarda quando, qual navegador, qual site e qual conta — nunca a senha. O arquivo inteiro é
//! protegido pela DPAPI, porque os sites visitados também são privados.
//!
//! Log of fills made by the extension (the plan asks the app to show it). Stores when, which
//! browser, which site and which account — never the password. The whole file is protected by
//! DPAPI, because visited sites are private too.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::atomic_write;
use crate::win::dpapi;

const LABEL: &[u8] = b"SigilVault fill log v1";
/// Entradas mantidas. / Entries kept.
pub const MAX_ENTRIES: usize = 200;

/// Um preenchimento. / One fill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillEntry {
    pub at: u64,
    pub browser: String,
    pub origin: String,
    pub title: String,
    pub username: String,
}

/// Registro em disco. / On-disk log.
pub struct FillLog {
    path: PathBuf,
    entries: Vec<FillEntry>,
}

impl FillLog {
    /// Lê o registro; arquivo ilegível vira registro vazio. / Reads the log; an unreadable file becomes an empty log.
    pub fn load(path: &Path) -> Self {
        let entries = std::fs::read(path)
            .ok()
            .and_then(|b| dpapi::unprotect_labeled(&b, LABEL))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path: path.to_owned(), entries }
    }

    /// Acrescenta e grava (melhor esforço). / Appends and saves (best effort).
    pub fn push(&mut self, entry: FillEntry) {
        self.entries.push(entry);
        let extra = self.entries.len().saturating_sub(MAX_ENTRIES);
        self.entries.drain(..extra);
        if let Ok(json) = serde_json::to_vec(&self.entries)
            && let Some(sealed) = dpapi::protect_labeled(&json, LABEL)
        {
            let _ = atomic_write(&self.path, &sealed);
        }
    }

    /// Do mais novo para o mais antigo. / Newest first.
    pub fn newest_first(&self) -> Vec<FillEntry> {
        self.entries.iter().rev().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_dir;

    fn entry(n: u64) -> FillEntry {
        FillEntry { at: n, browser: "Brave".into(), origin: "https://banco.example".into(), title: "Banco".into(), username: "ana".into() }
    }

    #[test]
    fn keeps_newest_and_is_protected() {
        let path = test_dir("filllog").join("fill-log.bin");
        let mut log = FillLog::load(&path);
        for n in 0..(MAX_ENTRIES as u64 + 5) {
            log.push(entry(n));
        }
        assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("banco"));
        let again = FillLog::load(&path).newest_first();
        assert_eq!(again.len(), MAX_ENTRIES);
        assert_eq!(again[0].at, MAX_ENTRIES as u64 + 4);
        std::fs::write(&path, b"lixo").unwrap();
        assert!(FillLog::load(&path).newest_first().is_empty());
    }
}
