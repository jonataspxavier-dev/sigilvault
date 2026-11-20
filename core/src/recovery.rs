//! Códigos de recuperação de uso único.
//! Single-use recovery codes.
//!
//! Formato: 26 símbolos base32 (Crockford) = 130 bits aleatórios, mais 1 dígito verificador
//! (Luhn mod 32), em grupos de 4: `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXC`.
//! Format: 26 base32 (Crockford) symbols = 130 random bits, plus 1 check digit
//! (Luhn mod 32), in groups of 4: `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXC`.
//!
//! Cada código gera, via HKDF, um identificador (para achar o slot sem testar os 10) e um par de
//! chaves X25519. O cofre guarda só a chave pública; a chave do cofre é "lacrada" para ela
//! (ECIES: X25519 + HKDF-SHA256 + XChaCha20-Poly1305). Assim, a chave do cofre pode ser trocada
//! sem precisar dos códigos, e quem abriu o cofre não ganha poder sobre versões futuras.
//! Each code yields, via HKDF, an identifier (to find its slot without trying all 10) and an
//! X25519 key pair. The vault stores only the public key; the vault key is "sealed" to it
//! (ECIES: X25519 + HKDF-SHA256 + XChaCha20-Poly1305). So the vault key can be rotated without
//! the codes, and someone who once opened the vault gains no power over future versions.

use std::fmt;

use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::{self, KEY_LEN, Sealed, SecretKey};
use crate::error::{Error, Result};

/// Quantidade de códigos gerados por vez. / Number of codes generated at a time.
pub const CODE_COUNT: usize = 10;
/// Símbolos de dados por código (26 × 5 = 130 bits). / Data symbols per code (26 × 5 = 130 bits).
const DATA_SYMBOLS: usize = 26;
// Verificado na compilação: pelo menos 128 bits. / Checked at compile time: at least 128 bits.
const _: () = assert!(DATA_SYMBOLS * 5 >= 128);
/// Alfabeto Crockford base32: sem I, L, O, U para evitar confusão.
/// Crockford base32 alphabet: no I, L, O, U to avoid confusion.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Tamanho do identificador público do slot. / Public slot identifier size.
pub const CODE_ID_LEN: usize = 16;

const INFO_ID: &[u8] = b"sigilvault v1 recovery code id";
const INFO_SECRET: &[u8] = b"sigilvault v1 recovery code x25519";
const INFO_WRAP: &[u8] = b"sigilvault v1 recovery wrap key";

/// Um código de recuperação. É apagado da memória ao sair de escopo.
/// A recovery code. Wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq, Zeroize)]
#[zeroize(drop)]
pub struct RecoveryCode {
    /// 26 valores de 0 a 31. / 26 values from 0 to 31.
    symbols: [u8; DATA_SYMBOLS],
}

impl RecoveryCode {
    /// Sorteia um código novo. Como 256 é múltiplo de 32, `byte % 32` não tem viés.
    /// Draws a new code. Since 256 is a multiple of 32, `byte % 32` is unbiased.
    pub fn generate() -> Result<Self> {
        let mut symbols = [0u8; DATA_SYMBOLS];
        crypto::fill_random(&mut symbols)?;
        for s in &mut symbols {
            *s %= 32;
        }
        Ok(Self { symbols })
    }

    /// Lê um código digitado. Aceita minúsculas, espaços e hífens, e troca O→0 e I/L→1.
    /// Parses a typed code. Accepts lowercase, spaces and hyphens, and maps O→0 and I/L→1.
    pub fn parse(input: &str) -> Result<Self> {
        let mut values = Vec::with_capacity(DATA_SYMBOLS + 1);
        for c in input.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            let v = ALPHABET.iter().position(|&a| a as char == c).ok_or(Error::MalformedRecoveryCode)?;
            values.push(v as u8);
        }
        if values.len() != DATA_SYMBOLS + 1 {
            values.zeroize();
            return Err(Error::MalformedRecoveryCode);
        }
        let mut symbols = [0u8; DATA_SYMBOLS];
        symbols.copy_from_slice(&values[..DATA_SYMBOLS]);
        let ok = check_symbol(&symbols) == values[DATA_SYMBOLS];
        values.zeroize();
        if ok { Ok(Self { symbols }) } else { Err(Error::MalformedRecoveryCode) }
    }

    /// Texto formatado para mostrar ao usuário (com o dígito verificador).
    /// Formatted text to show the user (with the check digit).
    pub fn to_display_string(&self) -> Zeroizing<String> {
        let mut all = self.symbols.to_vec();
        all.push(check_symbol(&self.symbols));
        let mut out = Zeroizing::new(String::with_capacity(34));
        for (i, v) in all.iter().enumerate() {
            if i > 0 && i % 4 == 0 {
                out.push('-');
            }
            out.push(ALPHABET[*v as usize] as char);
        }
        all.zeroize();
        out
    }

    /// Deriva o identificador do slot e o segredo X25519, usando o sal de recuperação do cofre.
    /// Derives the slot identifier and the X25519 secret, using the vault's recovery salt.
    pub(crate) fn derive(&self, salt: &[u8]) -> Result<RecoveryKeys> {
        let id = crypto::hkdf::<CODE_ID_LEN>(&self.symbols, salt, INFO_ID)?;
        let secret = crypto::hkdf::<32>(&self.symbols, salt, INFO_SECRET)?;
        Ok(RecoveryKeys { id: *id, secret: StaticSecret::from(*secret) })
    }
}

impl fmt::Debug for RecoveryCode {
    /// Nunca mostra o código em logs. / Never shows the code in logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(****)")
    }
}

/// Dígito verificador Luhn mod 32: detecta qualquer símbolo trocado e quase toda inversão de vizinhos.
/// Luhn mod 32 check digit: catches any single wrong symbol and almost every adjacent swap.
fn check_symbol(symbols: &[u8]) -> u8 {
    let mut factor = 2u32;
    let mut sum = 0u32;
    for &v in symbols.iter().rev() {
        let addend = factor * v as u32;
        sum += addend / 32 + addend % 32;
        factor = if factor == 2 { 1 } else { 2 };
    }
    ((32 - sum % 32) % 32) as u8
}

/// Chaves derivadas de um código. / Keys derived from a code.
pub(crate) struct RecoveryKeys {
    pub id: [u8; CODE_ID_LEN],
    pub secret: StaticSecret,
}

impl RecoveryKeys {
    pub fn public(&self) -> [u8; 32] {
        PublicKey::from(&self.secret).to_bytes()
    }
}

/// Resultado de lacrar a chave do cofre para um código.
/// Result of sealing the vault key to a code.
pub(crate) struct WrappedForCode {
    pub ephemeral_public: [u8; 32],
    pub sealed: Sealed,
}

fn wrap_key(shared: &[u8; 32], ephemeral_public: &[u8; 32], recipient_public: &[u8; 32]) -> Result<SecretKey> {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(ephemeral_public);
    salt[32..].copy_from_slice(recipient_public);
    crypto::hkdf::<KEY_LEN>(shared, &salt, INFO_WRAP)
}

/// Lacra a chave do cofre para a chave pública de um código (não precisa do código).
/// Seals the vault key to a code's public key (does not need the code).
pub(crate) fn wrap_for_code(recipient_public: &[u8; 32], vault_key: &[u8; KEY_LEN], aad: &[u8]) -> Result<WrappedForCode> {
    let ephemeral = StaticSecret::from(*crypto::random_key()?);
    let ephemeral_public = PublicKey::from(&ephemeral).to_bytes();
    let shared = ephemeral.diffie_hellman(&PublicKey::from(*recipient_public));
    if !shared.was_contributory() {
        return Err(Error::Crypto);
    }
    let kek = wrap_key(shared.as_bytes(), &ephemeral_public, recipient_public)?;
    let sealed = crypto::seal(&kek, vault_key, aad)?;
    Ok(WrappedForCode { ephemeral_public, sealed })
}

/// Abre a chave do cofre usando as chaves derivadas do código.
/// Opens the vault key using the keys derived from the code.
pub(crate) fn unwrap_with_code(keys: &RecoveryKeys, ephemeral_public: &[u8; 32], sealed: &Sealed, aad: &[u8]) -> Result<SecretKey> {
    let shared = keys.secret.diffie_hellman(&PublicKey::from(*ephemeral_public));
    if !shared.was_contributory() {
        return Err(Error::InvalidCredentials);
    }
    let kek = wrap_key(shared.as_bytes(), ephemeral_public, &keys.public())?;
    let plain = crypto::open(&kek, &sealed.nonce, &sealed.ciphertext, aad).map_err(|_| Error::InvalidCredentials)?;
    let key: [u8; KEY_LEN] = plain.as_slice().try_into().map_err(|_| Error::InvalidCredentials)?;
    Ok(Zeroizing::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_roundtrip() {
        for _ in 0..100 {
            let code = RecoveryCode::generate().unwrap();
            let text = code.to_display_string();
            assert_eq!(text.len(), 27 + 6);
            assert_eq!(RecoveryCode::parse(&text).unwrap(), code);
            // Minúsculas e sem hífens também valem. / Lowercase and no hyphens also work.
            let relaxed = text.replace('-', " ").to_lowercase();
            assert_eq!(RecoveryCode::parse(&relaxed).unwrap(), code);
        }
    }

    #[test]
    fn check_digit_catches_every_single_typo() {
        let code = RecoveryCode::generate().unwrap();
        let text: Vec<char> = code.to_display_string().replace('-', "").chars().collect();
        for i in 0..text.len() {
            for &a in ALPHABET {
                let a = a as char;
                if a == text[i] {
                    continue;
                }
                let mut typo = text.clone();
                typo[i] = a;
                let s: String = typo.into_iter().collect();
                assert!(matches!(RecoveryCode::parse(&s), Err(Error::MalformedRecoveryCode)), "{s}");
            }
        }
    }

    #[test]
    fn rejects_bad_length_and_chars() {
        assert!(RecoveryCode::parse("").is_err());
        assert!(RecoveryCode::parse("ABCD-EFGH").is_err());
        assert!(RecoveryCode::parse("UUUU-UUUU-UUUU-UUUU-UUUU-UUUU-UUU").is_err());
    }

    #[test]
    fn ambiguous_letters_are_normalized() {
        let code = RecoveryCode::generate().unwrap();
        let text = code.to_display_string().replace('0', "o").replace('1', "l");
        assert_eq!(RecoveryCode::parse(&text).unwrap(), code);
    }

    #[test]
    fn codes_are_distinct() {
        let a = RecoveryCode::generate().unwrap();
        let b = RecoveryCode::generate().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn wrap_and_unwrap_with_code() {
        let salt = [7u8; 32];
        let code = RecoveryCode::generate().unwrap();
        let keys = code.derive(&salt).unwrap();
        let vault_key = crypto::random_key().unwrap();
        let w = wrap_for_code(&keys.public(), &vault_key, b"aad").unwrap();
        let back = unwrap_with_code(&keys, &w.ephemeral_public, &w.sealed, b"aad").unwrap();
        assert_eq!(*back, *vault_key);

        let other = RecoveryCode::generate().unwrap().derive(&salt).unwrap();
        assert!(matches!(
            unwrap_with_code(&other, &w.ephemeral_public, &w.sealed, b"aad"),
            Err(Error::InvalidCredentials)
        ));
        assert!(unwrap_with_code(&keys, &w.ephemeral_public, &w.sealed, b"outro").is_err());
    }

    #[test]
    fn id_depends_on_vault_salt() {
        let code = RecoveryCode::generate().unwrap();
        assert_ne!(code.derive(&[1u8; 32]).unwrap().id, code.derive(&[2u8; 32]).unwrap().id);
    }

    #[test]
    fn debug_hides_code() {
        let code = RecoveryCode::generate().unwrap();
        assert_eq!(format!("{code:?}"), "RecoveryCode(****)");
    }
}
