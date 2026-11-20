//! Avaliação da senha mestra com zxcvbn.
//! Master password evaluation with zxcvbn.

use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;
use zxcvbn::{Score, zxcvbn};

use crate::error::{Error, Result};

/// Nota mínima do zxcvbn (0 a 4) para aceitar uma senha mestra. A nota 3 significa
/// "protegida contra ataque offline com hash lento", que é o nosso caso (Argon2id).
/// Minimum zxcvbn score (0 to 4) to accept a master password. Score 3 means
/// "safe against an offline slow-hash attack", which is our case (Argon2id).
pub const MIN_SCORE: Score = Score::Three;

/// Recusa senha mestra fraca. O nome de usuário conta como palavra conhecida pelo atacante.
/// Rejects a weak master password. The username counts as a word known to the attacker.
pub fn check_master_password(password: &str, username: &str) -> Result<()> {
    let estimate = zxcvbn(password, &[username]);
    if estimate.score() >= MIN_SCORE { Ok(()) } else { Err(Error::WeakMasterPassword) }
}

/// Normaliza a senha em Unicode NFC, para que "é" digitado de jeitos diferentes gere a mesma chave.
/// Normalizes the password to Unicode NFC, so "é" typed in different ways yields the same key.
pub(crate) fn normalize(password: &str) -> Zeroizing<String> {
    Zeroizing::new(password.nfc().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_passwords_rejected() {
        for p in ["", "123456", "senha123", "password", "joao2024", "qwertyuiop"] {
            assert!(check_master_password(p, "joao").is_err(), "{p}");
        }
    }

    #[test]
    fn strong_passphrase_accepted() {
        assert!(check_master_password("cavalo-trilho-nuvem-sete-farol-mesa", "joao").is_ok());
    }

    #[test]
    fn username_counts_against_password() {
        let user = "brunaferreiradasilva";
        assert!(check_master_password(user, user).is_err());
    }

    #[test]
    fn nfc_normalization() {
        // "é" composto (U+00E9) e decomposto (e + U+0301) viram a mesma coisa.
        // Composed "é" (U+00E9) and decomposed (e + U+0301) become the same.
        assert_eq!(*normalize("caf\u{e9}"), *normalize("cafe\u{301}"));
    }
}
