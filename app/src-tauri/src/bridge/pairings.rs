//! Extensões pareadas, salvas em `pairings.json`. A chave de cada pareamento é uma credencial:
//! fica protegida pela DPAPI (presa à conta do Windows) e só é aberta na memória.
//! Paired extensions, saved in `pairings.json`. Each pairing key is a credential: it is protected
//! by DPAPI (tied to the Windows account) and only opened in memory.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sigilvault_core::hex;
use zeroize::Zeroizing;

use crate::storage::atomic_write;
use crate::win::dpapi;

/// Rótulo DPAPI só destes dados. / DPAPI label only for this data.
const LABEL: &[u8] = b"SigilVault pairing keys v1";
/// Limite de pareamentos (evita crescer sem fim). / Pairing limit (avoids endless growth).
pub const MAX_PAIRINGS: usize = 20;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    id: String,
    caller: String,
    browser: String,
    created_at: u64,
    last_seen: u64,
    /// Chave protegida pela DPAPI, em hexadecimal. / DPAPI-protected key, in hex.
    key: String,
}

/// Um pareamento. / A pairing.
pub struct Pairing {
    pub id: [u8; 16],
    /// Origem da extensão, segundo o navegador. / Extension origin, according to the browser.
    pub caller: String,
    /// Nome do navegador, só para mostrar. / Browser name, display only.
    pub browser: String,
    pub created_at: u64,
    pub last_seen: u64,
    pub key: Zeroizing<[u8; 32]>,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Lista de pareamentos com o arquivo onde ela mora. / Pairing list with its file.
pub struct Pairings {
    path: PathBuf,
    list: Vec<Pairing>,
}

impl Pairings {
    /// Lê o arquivo. Entradas que a DPAPI não abre (outra conta, arquivo alterado) são ignoradas.
    /// Reads the file. Entries DPAPI cannot open (another account, altered file) are ignored.
    pub fn load(path: &Path) -> Self {
        let stored: Vec<Stored> = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let list = stored
            .into_iter()
            .filter_map(|s| {
                let id: [u8; 16] = hex::decode(&s.id)?.try_into().ok()?;
                let opened = Zeroizing::new(dpapi::unprotect_labeled(&hex::decode(&s.key)?, LABEL)?);
                let key: [u8; 32] = opened.as_slice().try_into().ok()?;
                Some(Pairing { id, caller: s.caller, browser: s.browser, created_at: s.created_at, last_seen: s.last_seen, key: Zeroizing::new(key) })
            })
            .take(MAX_PAIRINGS)
            .collect();
        Self { path: path.to_owned(), list }
    }

    fn save(&self) -> io::Result<()> {
        let stored = self
            .list
            .iter()
            .map(|p| {
                let key = dpapi::protect_labeled(p.key.as_slice(), LABEL).ok_or_else(|| io::Error::other("DPAPI"))?;
                Ok(Stored {
                    id: hex::encode(&p.id),
                    caller: p.caller.clone(),
                    browser: p.browser.clone(),
                    created_at: p.created_at,
                    last_seen: p.last_seen,
                    key: hex::encode(&key),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        atomic_write(&self.path, &serde_json::to_vec_pretty(&stored).map_err(io::Error::other)?)
    }

    pub fn list(&self) -> &[Pairing] {
        &self.list
    }

    pub fn is_full(&self) -> bool {
        self.list.len() >= MAX_PAIRINGS
    }

    /// Adiciona e grava. / Adds and saves.
    pub fn add(&mut self, pairing: Pairing) -> io::Result<()> {
        if self.is_full() {
            return Err(io::Error::other("limite de pareamentos / pairing limit"));
        }
        self.list.push(pairing);
        let result = self.save();
        if result.is_err() {
            self.list.pop();
        }
        result
    }

    /// Remove e grava. Devolve se existia. / Removes and saves. Returns whether it existed.
    pub fn remove(&mut self, id: &[u8; 16]) -> io::Result<bool> {
        let before = self.list.len();
        self.list.retain(|p| &p.id != id);
        if self.list.len() == before {
            return Ok(false);
        }
        self.save().map(|_| true)
    }

    /// Procura pelo id, exigindo a mesma origem que pareou. / Finds by id, requiring the same origin that paired.
    pub fn find(&self, id: &[u8; 16], caller: &str) -> Option<&Pairing> {
        self.list.iter().find(|p| &p.id == id && p.caller == caller)
    }

    /// Marca o último uso (gravado no máximo uma vez por hora). / Marks last use (saved at most hourly).
    pub fn touch(&mut self, id: &[u8; 16]) {
        let now = now_secs();
        if let Some(p) = self.list.iter_mut().find(|p| &p.id == id)
            && now.saturating_sub(p.last_seen) >= 3600
        {
            p.last_seen = now;
            let _ = self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_dir;

    fn pairing(n: u8, caller: &str) -> Pairing {
        Pairing { id: [n; 16], caller: caller.into(), browser: "Chrome".into(), created_at: 1, last_seen: 1, key: Zeroizing::new([n; 32]) }
    }

    #[test]
    fn saves_protected_and_reloads() {
        let dir = test_dir("pairings");
        let path = dir.join("pairings.json");
        let mut p = Pairings::load(&path);
        p.add(pairing(1, "chrome-extension://a/")).unwrap();
        p.add(pairing(2, "firefox:b")).unwrap();
        // A chave nunca aparece em claro no arquivo. / The key never appears in clear in the file.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&hex::encode(&[1u8; 32])));

        let p = Pairings::load(&path);
        assert_eq!(p.list().len(), 2);
        assert_eq!(*p.find(&[1; 16], "chrome-extension://a/").unwrap().key, [1u8; 32]);
        // Mesmo id, outra origem: não serve. / Same id, other origin: no match.
        assert!(p.find(&[1; 16], "firefox:b").is_none());
    }

    #[test]
    fn remove_limit_and_tampering() {
        let dir = test_dir("pairings2");
        let path = dir.join("pairings.json");
        let mut p = Pairings::load(&path);
        for n in 0..MAX_PAIRINGS as u8 {
            p.add(pairing(n, "c")).unwrap();
        }
        assert!(p.add(pairing(99, "c")).is_err());
        assert!(p.remove(&[3; 16]).unwrap());
        assert!(!p.remove(&[3; 16]).unwrap());
        assert_eq!(Pairings::load(&path).list().len(), MAX_PAIRINGS - 1);

        // Chave adulterada no arquivo: o pareamento some (não vira outra chave).
        // Tampered key in the file: the pairing disappears (it does not become another key).
        let mut stored: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let k = stored[0]["key"].as_str().unwrap().to_owned();
        let flipped = format!("{}{}", &k[..k.len() - 2], if k.ends_with("00") { "01" } else { "00" });
        stored[0]["key"] = serde_json::Value::String(flipped);
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert_eq!(Pairings::load(&path).list().len(), MAX_PAIRINGS - 2);
    }
}
