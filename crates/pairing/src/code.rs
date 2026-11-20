//! Código de pareamento: 8 símbolos Crockford base32 (40 bits), mostrado como `ABCD-EFGH`.
//! 40 bits bastam porque a OPAQUE só permite tentar online, e o app aceita 3 tentativas.
//!
//! Pairing code: 8 Crockford base32 symbols (40 bits), shown as `ABCD-EFGH`.
//! 40 bits are enough because OPAQUE only allows online guesses, and the app accepts 3 attempts.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::Error;

/// Quantidade de símbolos. / Number of symbols.
pub const LEN: usize = 8;

/// Alfabeto Crockford: sem I, L, O e U, que se confundem. / Crockford alphabet: no I, L, O, U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Código de pareamento (apagado da memória ao sair de uso). / Pairing code (wiped when dropped).
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct PairingCode([u8; LEN]);

/// Nunca mostra o código em logs. / Never shows the code in logs.
impl core::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PairingCode(***)")
    }
}

impl PairingCode {
    /// Sorteia um código. 32 símbolos = 5 bits exatos, então não há viés.
    /// Draws a code. 32 symbols = exactly 5 bits, so there is no bias.
    pub fn generate(fill: &mut impl FnMut(&mut [u8])) -> Self {
        let mut raw = [0u8; LEN];
        fill(&mut raw);
        let mut out = [0u8; LEN];
        for (o, r) in out.iter_mut().zip(raw.iter()) {
            *o = ALPHABET[usize::from(r & 31)];
        }
        raw.zeroize();
        Self(out)
    }

    /// Lê o que o usuário digitou: ignora hífens, espaços e maiúsculas/minúsculas, e aceita
    /// I/L no lugar de 1 e O no lugar de 0. / Reads what the user typed: ignores hyphens, spaces
    /// and case, and accepts I/L for 1 and O for 0.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let mut out = [0u8; LEN];
        let mut n = 0;
        for ch in input.chars() {
            let c = match ch.to_ascii_uppercase() {
                '-' | ' ' => continue,
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            };
            if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) || n == LEN {
                out.zeroize();
                return Err(Error::MalformedCode);
            }
            out[n] = c as u8;
            n += 1;
        }
        if n != LEN {
            out.zeroize();
            return Err(Error::MalformedCode);
        }
        Ok(Self(out))
    }

    /// Bytes canônicos (usados como "senha" na OPAQUE). / Canonical bytes (used as OPAQUE "password").
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Texto para mostrar: `ABCD-EFGH`. / Display text: `ABCD-EFGH`.
    pub fn to_display_string(&self) -> String {
        let s = core::str::from_utf8(&self.0).unwrap_or_default();
        format!("{}-{}", &s[..4], &s[4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_lenient_parsing() {
        let mut fill = |b: &mut [u8]| getrandom::fill(b).unwrap();
        for _ in 0..200 {
            let code = PairingCode::generate(&mut fill);
            let shown = code.to_display_string();
            assert_eq!(shown.len(), 9);
            assert_eq!(PairingCode::parse(&shown).unwrap(), code);
            assert_eq!(PairingCode::parse(&shown.to_lowercase()).unwrap(), code);
        }
        assert_eq!(PairingCode::parse("o1lI abcd").unwrap(), PairingCode::parse("0111-ABCD").unwrap());
    }

    #[test]
    fn rejects_malformed() {
        for bad in ["", "ABCD-EFG", "ABCD-EFGHJ", "ABCD-EFGU", "ABCD-EFG!", "ÁBCD-EFGH"] {
            assert_eq!(PairingCode::parse(bad), Err(Error::MalformedCode), "{bad}");
        }
    }
}
