//! Conversão de bytes para hexadecimal no JSON do arquivo (mais legível que listas de números).
//! Byte-to-hex conversion for the file's JSON (more readable than lists of numbers).

/// Codifica bytes em hexadecimal minúsculo.
/// Encodes bytes as lowercase hexadecimal.
pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    s
}

/// Decodifica hexadecimal; devolve `None` se for inválido.
/// Decodes hexadecimal; returns `None` if invalid.
pub fn decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    b.chunks_exact(2).map(|p| Some(val(p[0])? << 4 | val(p[1])?)).collect()
}

/// Serde para `Vec<u8>` como hexadecimal.
/// Serde for `Vec<u8>` as hexadecimal.
pub mod vec {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    /// Serializa. / Serializes.
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::encode(v))
    }
    /// Desserializa. / Deserializes.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        super::decode(&s).ok_or_else(|| D::Error::custom("hex inválido / invalid hex"))
    }
}

/// Serde para `[u8; N]` como hexadecimal.
/// Serde for `[u8; N]` as hexadecimal.
pub mod array {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    /// Serializa. / Serializes.
    pub fn serialize<S: Serializer, const N: usize>(v: &[u8; N], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::encode(v))
    }
    /// Desserializa. / Deserializes.
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(d: D) -> Result<[u8; N], D::Error> {
        let s = String::deserialize(d)?;
        let v = super::decode(&s).ok_or_else(|| D::Error::custom("hex inválido / invalid hex"))?;
        v.try_into().map_err(|_| D::Error::custom("tamanho errado / wrong length"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 0xab, 0xff];
        assert_eq!(super::encode(&data), "0001abff");
        assert_eq!(super::decode("0001ABff").unwrap(), data);
        assert!(super::decode("0g").is_none());
        assert!(super::decode("abc").is_none());
    }
}
