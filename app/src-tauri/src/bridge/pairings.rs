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
use crate::platform::secret;

/// Rótulo DPAPI só destes dados. / DPAPI label only for this data.
const LABEL: &[u8] = b"SigilVault pairing keys v1";
/// Rótulo DPAPI da lista de sites permitidos (os sites visitados também são privados).
/// DPAPI label of the allowed sites list (visited sites are private too).
const SITES_LABEL: &[u8] = b"SigilVault allowed sites v1";
/// Limite de sites permitidos por pareamento. / Allowed sites limit per pairing.
pub const MAX_SITES: usize = 500;
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
    /// Sites onde o preenchimento foi permitido para sempre (lista JSON protegida pela DPAPI, em hex).
    /// Sites where filling was allowed for good (DPAPI-protected JSON list, in hex).
    #[serde(default)]
    sites: String,
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
    /// Origens (canônicas) liberadas para preencher sem perguntar. / Origins (canonical) allowed to fill without asking.
    pub sites: Vec<String>,
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
                let opened = Zeroizing::new(secret::unprotect_labeled(&hex::decode(&s.key)?, LABEL)?);
                let key: [u8; 32] = opened.as_slice().try_into().ok()?;
                // Lista ilegível = nenhum site liberado (nunca o contrário). / Unreadable list = no site allowed (never the reverse).
                let sites = hex::decode(&s.sites)
                    .and_then(|b| secret::unprotect_labeled(&b, SITES_LABEL))
                    .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
                    .unwrap_or_default();
                Some(Pairing {
                    id,
                    caller: s.caller,
                    browser: s.browser,
                    created_at: s.created_at,
                    last_seen: s.last_seen,
                    key: Zeroizing::new(key),
                    sites,
                })
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
                let key = secret::protect_labeled(p.key.as_slice(), LABEL).ok_or_else(|| io::Error::other("DPAPI"))?;
                let sites_json = serde_json::to_vec(&p.sites).map_err(io::Error::other)?;
                let sites = secret::protect_labeled(&sites_json, SITES_LABEL).ok_or_else(|| io::Error::other("DPAPI"))?;
                Ok(Stored {
                    id: hex::encode(&p.id),
                    caller: p.caller.clone(),
                    browser: p.browser.clone(),
                    created_at: p.created_at,
                    last_seen: p.last_seen,
                    key: hex::encode(&key),
                    sites: hex::encode(&sites),
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

    /// O site já foi liberado para este pareamento? / Was the site already allowed for this pairing?
    pub fn is_allowed(&self, id: &[u8; 16], origin: &str) -> bool {
        self.list.iter().any(|p| &p.id == id && p.sites.iter().any(|s| s == origin))
    }

    /// Libera um site para sempre (até ser revogado). / Allows a site for good (until revoked).
    pub fn allow_site(&mut self, id: &[u8; 16], origin: &str) -> io::Result<()> {
        let Some(p) = self.list.iter_mut().find(|p| &p.id == id) else { return Ok(()) };
        if p.sites.iter().any(|s| s == origin) {
            return Ok(());
        }
        if p.sites.len() >= MAX_SITES {
            p.sites.remove(0);
        }
        p.sites.push(origin.to_owned());
        self.save()
    }

    /// Revoga um site. / Revokes a site.
    pub fn revoke_site(&mut self, id: &[u8; 16], origin: &str) -> io::Result<bool> {
        let Some(p) = self.list.iter_mut().find(|p| &p.id == id) else { return Ok(false) };
        let before = p.sites.len();
        p.sites.retain(|s| s != origin);
        if p.sites.len() == before {
            return Ok(false);
        }
        self.save().map(|_| true)
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
        Pairing { id: [n; 16], caller: caller.into(), browser: "Chrome".into(), created_at: 1, last_seen: 1, key: Zeroizing::new([n; 32]), sites: Vec::new() }
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
    fn allowed_sites_are_private_and_per_pairing() {
        let dir = test_dir("pairings-sites");
        let path = dir.join("pairings.json");
        let mut p = Pairings::load(&path);
        p.add(pairing(1, "c")).unwrap();
        p.add(pairing(2, "c")).unwrap();
        p.allow_site(&[1; 16], "https://banco.example").unwrap();
        p.allow_site(&[1; 16], "https://banco.example").unwrap();
        // O site visitado não aparece em claro no arquivo. / The visited site does not appear in clear in the file.
        assert!(!std::fs::read_to_string(&path).unwrap().contains("banco"));

        let mut p = Pairings::load(&path);
        assert!(p.is_allowed(&[1; 16], "https://banco.example"));
        assert!(!p.is_allowed(&[2; 16], "https://banco.example"));
        assert!(!p.is_allowed(&[1; 16], "http://banco.example"));
        assert_eq!(p.list()[0].sites.len(), 1);
        assert!(p.revoke_site(&[1; 16], "https://banco.example").unwrap());
        assert!(!Pairings::load(&path).is_allowed(&[1; 16], "https://banco.example"));
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
