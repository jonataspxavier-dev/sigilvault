//! Blocos básicos de criptografia. Só chamamos bibliotecas auditadas; nada é inventado aqui.
//! Basic cryptographic building blocks. We only call audited libraries; nothing is invented here.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// Tamanho de chaves simétricas (256 bits).
/// Symmetric key size (256 bits).
pub const KEY_LEN: usize = 32;
/// Tamanho do nonce do XChaCha20-Poly1305 (192 bits): grande o bastante para sortear com segurança.
/// XChaCha20-Poly1305 nonce size (192 bits): large enough to pick at random safely.
pub const NONCE_LEN: usize = 24;
/// Tamanho do sal do Argon2.
/// Argon2 salt size.
pub const SALT_LEN: usize = 32;

/// Chave simétrica que é apagada da memória ao sair de escopo.
/// Symmetric key wiped from memory when dropped.
pub type SecretKey = Zeroizing<[u8; KEY_LEN]>;

/// Preenche `buf` com bytes do gerador seguro do sistema operacional.
/// Fills `buf` with bytes from the operating system's secure generator.
pub fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|_| Error::Rng)
}

/// Devolve um vetor de bytes aleatórios de tamanho fixo.
/// Returns a fixed-size array of random bytes.
pub fn random_array<const N: usize>() -> Result<[u8; N]> {
    let mut out = [0u8; N];
    fill_random(&mut out)?;
    Ok(out)
}

/// Gera uma chave aleatória nova.
/// Generates a new random key.
pub fn random_key() -> Result<SecretKey> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    fill_random(key.as_mut())?;
    Ok(key)
}

/// Parâmetros do Argon2id, registrados em cada slot de senha.
/// Argon2id parameters, recorded in each password slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memória em KiB. / Memory in KiB.
    pub memory_kib: u32,
    /// Número de passadas. / Number of passes.
    pub iterations: u32,
    /// Paralelismo (faixas). / Parallelism (lanes).
    pub parallelism: u32,
}

impl KdfParams {
    /// Padrão do projeto: 128 MiB, 3 passadas, 1 faixa.
    /// Project default: 128 MiB, 3 passes, 1 lane.
    pub const DEFAULT: Self = Self { memory_kib: 128 * 1024, iterations: 3, parallelism: 1 };

    /// Mínimo obrigatório: nenhum cofre é aberto ou criado com menos que isso.
    /// Mandatory minimum: no vault is opened or created with less than this.
    pub const MINIMUM: Self = Self { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 };

    /// Máximo aceito: impede que um arquivo malicioso trave o PC pedindo memória absurda.
    /// Accepted maximum: stops a malicious file from freezing the PC with absurd memory requests.
    pub const MAXIMUM: Self = Self { memory_kib: 2 * 1024 * 1024, iterations: 64, parallelism: 16 };

    /// Confere se os parâmetros estão dentro dos limites.
    /// Checks that the parameters are within bounds.
    pub fn validate(&self) -> Result<()> {
        let ok = (Self::MINIMUM.memory_kib..=Self::MAXIMUM.memory_kib).contains(&self.memory_kib)
            && (Self::MINIMUM.iterations..=Self::MAXIMUM.iterations).contains(&self.iterations)
            && (Self::MINIMUM.parallelism..=Self::MAXIMUM.parallelism).contains(&self.parallelism);
        if ok { Ok(()) } else { Err(Error::WeakKdfParams) }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Deriva uma chave de 256 bits da senha com Argon2id.
/// Derives a 256-bit key from the password with Argon2id.
pub fn derive_password_key(password: &[u8], salt: &[u8], params: &KdfParams) -> Result<SecretKey> {
    params.validate()?;
    let p = Params::new(params.memory_kib, params.iterations, params.parallelism, Some(KEY_LEN))
        .map_err(|_| Error::WeakKdfParams)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon.hash_password_into(password, salt, out.as_mut()).map_err(|_| Error::Crypto)?;
    Ok(out)
}

/// Deriva `N` bytes com HKDF-SHA256. `info` separa os usos (nunca reutilizar o mesmo `info`).
/// Derives `N` bytes with HKDF-SHA256. `info` separates uses (never reuse the same `info`).
pub fn hkdf<const N: usize>(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<Zeroizing<[u8; N]>> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = Zeroizing::new([0u8; N]);
    hk.expand(info, out.as_mut()).map_err(|_| Error::Crypto)?;
    Ok(out)
}

/// Texto cifrado com o nonce usado.
/// Ciphertext together with the nonce used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    /// Nonce aleatório de 192 bits. / Random 192-bit nonce.
    #[serde(with = "crate::hex::array")]
    pub nonce: [u8; NONCE_LEN],
    /// Texto cifrado + etiqueta de 16 bytes. / Ciphertext + 16-byte tag.
    #[serde(with = "crate::hex::vec")]
    pub ciphertext: Vec<u8>,
}

/// Criptografa com XChaCha20-Poly1305 e nonce aleatório. `aad` é autenticado mas não cifrado.
/// Encrypts with XChaCha20-Poly1305 and a random nonce. `aad` is authenticated, not encrypted.
pub fn seal(key: &[u8; KEY_LEN], plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    let nonce: [u8; NONCE_LEN] = random_array()?;
    let ciphertext = seal_with_nonce(key, &nonce, plaintext, aad)?;
    Ok(Sealed { nonce, ciphertext })
}

/// Criptografa com um nonce já sorteado (usado quando o nonce precisa estar no cabeçalho antes).
/// Encrypts with an already-drawn nonce (used when the nonce must be in the header beforehand).
pub fn seal_with_nonce(key: &[u8; KEY_LEN], nonce: &[u8; NONCE_LEN], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .encrypt(&XNonce::from(*nonce), Payload { msg: plaintext, aad })
        .map_err(|_| Error::Crypto)
}

/// Descriptografa e verifica. Falha de verificação vira [`Error::Tampered`]; quem chama pode
/// converter para [`Error::InvalidCredentials`] quando o erro vier de uma chave errada.
/// Decrypts and verifies. Verification failure becomes [`Error::Tampered`]; callers may map it
/// to [`Error::InvalidCredentials`] when the failure comes from a wrong key.
pub fn open(key: &[u8; KEY_LEN], nonce: &[u8; NONCE_LEN], ciphertext: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .decrypt(&XNonce::from(*nonce), Payload { msg: ciphertext, aad })
        .map(Zeroizing::new)
        .map_err(|_| Error::Tampered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = random_key().unwrap();
        let s = seal(&key, b"segredo", b"aad").unwrap();
        assert_eq!(&*open(&key, &s.nonce, &s.ciphertext, b"aad").unwrap(), b"segredo");
    }

    #[test]
    fn open_rejects_wrong_aad_key_or_bitflip() {
        let key = random_key().unwrap();
        let other = random_key().unwrap();
        let s = seal(&key, b"segredo", b"aad").unwrap();
        assert!(open(&key, &s.nonce, &s.ciphertext, b"outro").is_err());
        assert!(open(&other, &s.nonce, &s.ciphertext, b"aad").is_err());
        let mut flipped = s.ciphertext.clone();
        flipped[0] ^= 1;
        assert!(open(&key, &s.nonce, &flipped, b"aad").is_err());
    }

    #[test]
    fn kdf_params_bounds() {
        assert!(KdfParams::DEFAULT.validate().is_ok());
        assert!(KdfParams::MINIMUM.validate().is_ok());
        let weak = KdfParams { memory_kib: 1024, ..KdfParams::DEFAULT };
        assert!(matches!(weak.validate(), Err(Error::WeakKdfParams)));
        let huge = KdfParams { memory_kib: u32::MAX, ..KdfParams::DEFAULT };
        assert!(huge.validate().is_err());
        assert!(derive_password_key(b"x", &[0u8; SALT_LEN], &weak).is_err());
    }

    #[test]
    fn argon2_is_deterministic_and_salted() {
        let p = KdfParams::MINIMUM;
        let a = derive_password_key(b"senha", &[1u8; SALT_LEN], &p).unwrap();
        let b = derive_password_key(b"senha", &[1u8; SALT_LEN], &p).unwrap();
        let c = derive_password_key(b"senha", &[2u8; SALT_LEN], &p).unwrap();
        assert_eq!(*a, *b);
        assert_ne!(*a, *c);
    }
}
