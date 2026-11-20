//! Formato binário do arquivo `.sigil` (versão 1).
//! Binary format of the `.sigil` file (version 1).
//!
//! ```text
//! [0..8)    "SIGILVLT"                        assinatura / magic
//! [8..10)   versão do formato (u16 LE)        format version (u16 LE)
//! [10..14)  tamanho do cabeçalho (u32 LE)     header length (u32 LE)
//! [14..H)   cabeçalho JSON (slots, parâmetros, revisão, nonce do corpo)
//!           JSON header (slots, parameters, revision, body nonce)
//! [H..)     corpo cifrado com XChaCha20-Poly1305; AAD = bytes [0..H)
//!           body encrypted with XChaCha20-Poly1305; AAD = bytes [0..H)
//! ```
//!
//! Como o AAD do corpo cobre o prefixo e o cabeçalho inteiros, qualquer alteração neles é detectada.
//! Since the body AAD covers the whole prefix and header, any change to them is detected.

use serde::{Deserialize, Serialize};

use crate::crypto::{KdfParams, NONCE_LEN, SALT_LEN, Sealed};
use crate::error::{Error, Result};
use crate::recovery::{CODE_COUNT, CODE_ID_LEN};

/// Assinatura no início do arquivo. / Magic bytes at the start of the file.
pub const MAGIC: &[u8; 8] = b"SIGILVLT";
/// Versão atual do formato. / Current format version.
pub const FORMAT_VERSION: u16 = 1;
/// Tamanho do identificador do cofre. / Vault identifier size.
pub const VAULT_ID_LEN: usize = 16;

const PREFIX_LEN: usize = 8 + 2 + 4;
/// Limite do cabeçalho: evita ler JSON gigante de um arquivo malicioso.
/// Header limit: avoids parsing huge JSON from a malicious file.
const MAX_HEADER_LEN: usize = 64 * 1024;
/// Limite do arquivo inteiro. / Whole-file limit.
pub const MAX_FILE_LEN: usize = 256 * 1024 * 1024;
/// Etiqueta do Poly1305. / Poly1305 tag size.
const TAG_LEN: usize = 16;

/// Cabeçalho do arquivo (não cifrado, mas autenticado).
/// File header (not encrypted, but authenticated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    #[serde(with = "crate::hex::array")]
    pub vault_id: [u8; VAULT_ID_LEN],
    /// Contador de gravações, para detectar "voltar versão".
    /// Save counter, to detect rollback.
    pub revision: u64,
    /// Sal usado para derivar os identificadores e chaves dos códigos.
    /// Salt used to derive the codes' identifiers and keys.
    #[serde(with = "crate::hex::array")]
    pub recovery_salt: [u8; SALT_LEN],
    #[serde(with = "crate::hex::array")]
    pub body_nonce: [u8; NONCE_LEN],
    pub password_slot: PasswordSlot,
    pub recovery_slots: Vec<RecoverySlot>,
}

/// Slot que guarda a chave do cofre trancada pela senha mestra.
/// Slot holding the vault key locked by the master password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordSlot {
    pub kdf: KdfParams,
    #[serde(with = "crate::hex::array")]
    pub salt: [u8; SALT_LEN],
    pub wrapped: Sealed,
}

/// Slot que guarda a chave do cofre lacrada para um código de recuperação.
/// Slot holding the vault key sealed to a recovery code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoverySlot {
    #[serde(with = "crate::hex::array")]
    pub id: [u8; CODE_ID_LEN],
    #[serde(with = "crate::hex::array")]
    pub ephemeral_public: [u8; 32],
    pub wrapped: Sealed,
}

/// AAD de um slot de senha: amarra o slot ao cofre e aos seus parâmetros.
/// Password slot AAD: binds the slot to the vault and its parameters.
pub(crate) fn password_slot_aad(vault_id: &[u8; VAULT_ID_LEN], kdf: &KdfParams, salt: &[u8; SALT_LEN]) -> Vec<u8> {
    let mut aad = b"sigilvault v1 password slot".to_vec();
    aad.extend_from_slice(vault_id);
    aad.extend_from_slice(&kdf.memory_kib.to_le_bytes());
    aad.extend_from_slice(&kdf.iterations.to_le_bytes());
    aad.extend_from_slice(&kdf.parallelism.to_le_bytes());
    aad.extend_from_slice(salt);
    aad
}

/// AAD de um slot de código: amarra o slot ao cofre e ao identificador do código.
/// Recovery slot AAD: binds the slot to the vault and the code identifier.
pub(crate) fn recovery_slot_aad(vault_id: &[u8; VAULT_ID_LEN], id: &[u8; CODE_ID_LEN]) -> Vec<u8> {
    let mut aad = b"sigilvault v1 recovery slot".to_vec();
    aad.extend_from_slice(vault_id);
    aad.extend_from_slice(id);
    aad
}

/// Monta prefixo + cabeçalho. O resultado é também o AAD do corpo.
/// Builds prefix + header. The result is also the body AAD.
pub(crate) fn encode_header(header: &Header) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(header).map_err(|_| Error::InvalidFormat("cabeçalho / header"))?;
    if json.len() > MAX_HEADER_LEN {
        return Err(Error::InvalidFormat("cabeçalho grande demais / header too large"));
    }
    let mut out = Vec::with_capacity(PREFIX_LEN + json.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Arquivo dividido em partes. / File split into parts.
pub(crate) struct ParsedFile<'a> {
    pub header: Header,
    /// Prefixo + cabeçalho, exatamente como estavam no arquivo (AAD do corpo).
    /// Prefix + header, exactly as in the file (body AAD).
    pub aad: &'a [u8],
    pub body_ciphertext: &'a [u8],
}

/// Lê e valida a estrutura do arquivo, sem descriptografar nada.
/// Reads and validates the file structure, without decrypting anything.
pub(crate) fn parse(bytes: &[u8]) -> Result<ParsedFile<'_>> {
    if bytes.len() > MAX_FILE_LEN {
        return Err(Error::InvalidFormat("arquivo grande demais / file too large"));
    }
    if bytes.len() < PREFIX_LEN || &bytes[..8] != MAGIC {
        return Err(Error::InvalidFormat("não é um cofre SigilVault / not a SigilVault vault"));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let header_len = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    if header_len > MAX_HEADER_LEN || bytes.len() < PREFIX_LEN + header_len + TAG_LEN {
        return Err(Error::InvalidFormat("tamanho do cabeçalho / header length"));
    }
    let end = PREFIX_LEN + header_len;
    let header: Header = serde_json::from_slice(&bytes[PREFIX_LEN..end])
        .map_err(|_| Error::InvalidFormat("cabeçalho ilegível / unreadable header"))?;

    header.password_slot.kdf.validate()?;
    if header.recovery_slots.len() > CODE_COUNT {
        return Err(Error::InvalidFormat("slots demais / too many slots"));
    }
    let mut ids: Vec<_> = header.recovery_slots.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != header.recovery_slots.len() {
        return Err(Error::InvalidFormat("slots repetidos / duplicate slots"));
    }
    Ok(ParsedFile { header, aad: &bytes[..end], body_ciphertext: &bytes[end..] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Header {
        Header {
            vault_id: [1; VAULT_ID_LEN],
            revision: 3,
            recovery_salt: [2; SALT_LEN],
            body_nonce: [3; NONCE_LEN],
            password_slot: PasswordSlot {
                kdf: KdfParams::MINIMUM,
                salt: [4; SALT_LEN],
                wrapped: Sealed { nonce: [5; NONCE_LEN], ciphertext: vec![6; 48] },
            },
            recovery_slots: vec![],
        }
    }

    fn file(header: &Header) -> Vec<u8> {
        let mut bytes = encode_header(header).unwrap();
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    #[test]
    fn header_roundtrip() {
        let h = sample();
        let bytes = file(&h);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.header, h);
        assert_eq!(parsed.body_ciphertext.len(), 32);
    }

    #[test]
    fn rejects_bad_magic_version_and_truncation() {
        let bytes = file(&sample());
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(matches!(parse(&bad), Err(Error::InvalidFormat(_))));
        let mut v2 = bytes.clone();
        v2[8] = 2;
        assert!(matches!(parse(&v2), Err(Error::UnsupportedVersion(2))));
        for len in 0..bytes.len() - 16 {
            assert!(parse(&bytes[..len]).is_err(), "len {len}");
        }
    }

    #[test]
    fn rejects_weak_kdf_in_file() {
        let mut h = sample();
        h.password_slot.kdf.memory_kib = 8;
        assert!(matches!(parse(&file(&h)), Err(Error::WeakKdfParams)));
    }

    #[test]
    fn rejects_duplicate_recovery_slots() {
        let mut h = sample();
        let slot = RecoverySlot {
            id: [9; CODE_ID_LEN],
            ephemeral_public: [0; 32],
            wrapped: Sealed { nonce: [0; NONCE_LEN], ciphertext: vec![0; 48] },
        };
        h.recovery_slots = vec![slot.clone(), slot];
        assert!(parse(&file(&h)).is_err());
    }
}
