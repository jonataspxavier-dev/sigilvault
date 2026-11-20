//! Canal criptografado de uma conexão, a partir da chave de pareamento.
//! Encrypted channel of one connection, from the pairing key.
//!
//! - Cada lado sorteia 32 bytes (nonce de sessão); as chaves saem de
//!   HKDF-SHA256(sal = nonce_extensão || nonce_app, chave = chave de pareamento), uma por direção.
//!   Each side draws 32 bytes (session nonce); keys come from
//!   HKDF-SHA256(salt = extension_nonce || app_nonce, key = pairing key), one per direction.
//! - AES-256-GCM, nonce de 12 bytes = 4 zeros + contador de 64 bits (big-endian). Mensagem
//!   repetida, perdida ou fora de ordem é recusada. / AES-256-GCM, 12-byte nonce = 4 zeros +
//!   64-bit counter (big-endian). Repeated, lost or out-of-order messages are refused.
//! - A extensão faz o mesmo com WebCrypto (extension/src/channel.ts); o teste com vetor fixo
//!   garante que os dois batem. / The extension does the same with WebCrypto
//!   (extension/src/channel.ts); the fixed-vector test makes sure both match.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::Error;

const INFO_EXT_TO_APP: &[u8] = b"SigilVault session v1 extension->app";
const INFO_APP_TO_EXT: &[u8] = b"SigilVault session v1 app->extension";
const AAD: &[u8] = b"SigilVault channel v1";

/// Qual lado do canal somos. / Which side of the channel we are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    App,
    Extension,
}

/// Canal de uma conexão. / Channel of one connection.
pub struct Channel {
    send: Aes256Gcm,
    recv: Aes256Gcm,
    send_counter: u64,
    recv_counter: u64,
}

impl Channel {
    /// Deriva as duas chaves da conexão. / Derives both keys of the connection.
    pub fn new(pairing_key: &[u8; 32], extension_nonce: &[u8; 32], app_nonce: &[u8; 32], role: Role) -> Self {
        let mut salt = [0u8; 64];
        salt[..32].copy_from_slice(extension_nonce);
        salt[32..].copy_from_slice(app_nonce);
        let hk = Hkdf::<Sha256>::new(Some(&salt), pairing_key);
        let mut e2a = [0u8; 32];
        let mut a2e = [0u8; 32];
        hk.expand(INFO_EXT_TO_APP, &mut e2a).expect("32 bytes");
        hk.expand(INFO_APP_TO_EXT, &mut a2e).expect("32 bytes");
        let (send, recv) = match role {
            Role::App => (&a2e, &e2a),
            Role::Extension => (&e2a, &a2e),
        };
        let channel = Self {
            send: Aes256Gcm::new(send.into()),
            recv: Aes256Gcm::new(recv.into()),
            send_counter: 0,
            recv_counter: 0,
        };
        e2a.zeroize();
        a2e.zeroize();
        channel
    }

    fn nonce(counter: u64) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&counter.to_be_bytes());
        n
    }

    /// Criptografa a próxima mensagem. / Encrypts the next message.
    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let nonce = Self::nonce(self.send_counter);
        self.send_counter += 1;
        self.send
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: AAD })
            .expect("AES-GCM não falha ao criptografar / AES-GCM does not fail to encrypt")
    }

    /// Abre a próxima mensagem; qualquer falha deve encerrar a conexão.
    /// Opens the next message; any failure must end the connection.
    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = Self::nonce(self.recv_counter);
        let plain = self
            .recv
            .decrypt(Nonce::from_slice(&nonce), Payload { msg: ciphertext, aad: AAD })
            .map_err(|_| Error::Rejected)?;
        self.recv_counter += 1;
        Ok(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Channel, Channel) {
        let key = [1u8; 32];
        let (e, a) = ([2u8; 32], [3u8; 32]);
        (Channel::new(&key, &e, &a, Role::Extension), Channel::new(&key, &e, &a, Role::App))
    }

    #[test]
    fn both_directions_work() {
        let (mut ext, mut app) = pair();
        for i in 0..5u8 {
            let c = ext.seal(&[i; 10]);
            assert_eq!(app.open(&c).unwrap(), vec![i; 10]);
            let c = app.seal(b"ok");
            assert_eq!(ext.open(&c).unwrap(), b"ok");
        }
    }

    #[test]
    fn replay_reorder_and_tamper_are_refused() {
        let (mut ext, mut app) = pair();
        let first = ext.seal(b"um");
        let second = ext.seal(b"dois");
        // Fora de ordem / Out of order
        assert!(app.open(&second).is_err());
        assert_eq!(app.open(&first).unwrap(), b"um");
        // Repetida / Replayed
        assert!(app.open(&first).is_err());
        // Adulterada / Tampered
        let mut bad = second.clone();
        bad[0] ^= 1;
        assert!(app.open(&bad).is_err());
        assert_eq!(app.open(&second).unwrap(), b"dois");
        // Refletida de volta (a mesma chave nas duas direções seria um erro grave).
        // Reflected back (the same key in both directions would be a serious bug).
        let mine = ext.seal(b"eco");
        assert!(ext.open(&mine).is_err());
    }

    #[test]
    fn other_session_nonces_do_not_open() {
        let key = [1u8; 32];
        let mut ext = Channel::new(&key, &[2; 32], &[3; 32], Role::Extension);
        let mut app = Channel::new(&key, &[2; 32], &[4; 32], Role::App);
        assert!(app.open(&ext.seal(b"x")).is_err());
    }

    /// Vetor fixo: o mesmo valor é conferido no teste da extensão (WebCrypto).
    /// Fixed vector: the same value is checked in the extension's test (WebCrypto).
    #[test]
    fn fixed_vector_matches_extension() {
        let key: [u8; 32] = core::array::from_fn(|i| i as u8);
        let e: [u8; 32] = core::array::from_fn(|i| 0x40 + i as u8);
        let a: [u8; 32] = core::array::from_fn(|i| 0x80 + i as u8);
        let mut ext = Channel::new(&key, &e, &a, Role::Extension);
        let mut app = Channel::new(&key, &e, &a, Role::App);
        let c1 = ext.seal(b"{\"op\":\"status\"}");
        let c2 = app.seal(b"{\"locked\":true}");
        assert_eq!(hex(&c1), VECTOR_EXT_TO_APP);
        assert_eq!(hex(&c2), VECTOR_APP_TO_EXT);
    }

    // Calculados de forma independente com WebCrypto (Node) e conferidos aqui.
    // Computed independently with WebCrypto (Node) and checked here.
    const VECTOR_EXT_TO_APP: &str = "329498716e49bc206520b1628feba923d6439fc4bf6e8346c3409b74df0321";
    const VECTOR_APP_TO_EXT: &str = "120bcafbe19f2bc94cb64873e4e066e602f09b93a3e758073dc1e7cd00186f";

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}
