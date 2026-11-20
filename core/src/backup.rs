//! Formato do arquivo de backup local (`.sigilbak`).
//! Local backup file format (`.sigilbak`).
//!
//! ```text
//! [0..8)    "SIGILBAK"
//! [8..10)   versão (u16 LE) / version (u16 LE)
//! [10..14)  tamanho do cabeçalho (u32 LE) / header length (u32 LE)
//! [14..H)   cabeçalho JSON (cofre, data, revisão, nonce) / JSON header (vault, date, revision, nonce)
//! [H..)     conteúdo cifrado com a chave de backup; AAD = bytes [0..H)
//!           content encrypted with the backup key; AAD = bytes [0..H)
//! ```
//!
//! Diferente do cofre, o backup **não tem slots**: a chave dele fica dentro do cofre atual.
//! Unlike the vault, a backup has **no slots**: its key lives inside the current vault.

use serde::{Deserialize, Serialize};

use crate::crypto::NONCE_LEN;
use crate::error::{Error, Result};
use crate::format::VAULT_ID_LEN;

const MAGIC: &[u8; 8] = b"SIGILBAK";
const VERSION: u16 = 1;
const PREFIX_LEN: usize = 14;
const MAX_HEADER_LEN: usize = 4096;
/// Limite do arquivo de backup. / Backup file limit.
pub const MAX_BACKUP_LEN: usize = 256 * 1024 * 1024;

/// Cabeçalho do backup (autenticado). / Backup header (authenticated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupHeader {
    #[serde(with = "crate::hex::array")]
    pub vault_id: [u8; VAULT_ID_LEN],
    /// Quando o backup foi feito (segundos Unix). / When the backup was made (Unix seconds).
    pub created_at: u64,
    /// Revisão do cofre no momento do backup. / Vault revision at backup time.
    pub revision: u64,
    #[serde(with = "crate::hex::array")]
    pub nonce: [u8; NONCE_LEN],
}

/// Monta prefixo + cabeçalho (também é o AAD). / Builds prefix + header (also the AAD).
pub(crate) fn encode_header(header: &BackupHeader) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(header).map_err(|_| Error::InvalidFormat("backup"))?;
    let mut out = Vec::with_capacity(PREFIX_LEN + json.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Separa cabeçalho, AAD e texto cifrado. / Splits header, AAD and ciphertext.
pub(crate) fn parse(bytes: &[u8]) -> Result<(BackupHeader, &[u8], &[u8])> {
    if bytes.len() > MAX_BACKUP_LEN || bytes.len() < PREFIX_LEN || &bytes[..8] != MAGIC {
        return Err(Error::InvalidFormat("não é um backup / not a backup"));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let len = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    if len > MAX_HEADER_LEN || bytes.len() < PREFIX_LEN + len + 16 {
        return Err(Error::InvalidFormat("cabeçalho do backup / backup header"));
    }
    let end = PREFIX_LEN + len;
    let header: BackupHeader =
        serde_json::from_slice(&bytes[PREFIX_LEN..end]).map_err(|_| Error::InvalidFormat("cabeçalho do backup / backup header"))?;
    Ok((header, &bytes[..end], &bytes[end..]))
}
