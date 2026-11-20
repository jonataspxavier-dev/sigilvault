//! Molduras: 4 bytes de tamanho (little-endian, como o Native Messaging no Windows) + conteúdo.
//! Frames: 4-byte length (little-endian, like Native Messaging on Windows) + content.

use std::io::{self, Read, Write};

/// Lê uma mensagem. `Ok(None)` = o outro lado fechou entre mensagens.
/// Reads one message. `Ok(None)` = the other side closed between messages.
pub fn read(mut r: impl Read, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "mensagem grande demais / message too large"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Escreve uma mensagem inteira de uma vez. / Writes a whole message at once.
pub fn write(mut w: impl Write, data: &[u8], max: usize) -> io::Result<()> {
    if data.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "mensagem grande demais / message too large"));
    }
    let mut buf = Vec::with_capacity(4 + data.len());
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
    w.write_all(&buf)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_limits() {
        let mut buf = Vec::new();
        write(&mut buf, b"{\"t\":\"hello\"}", 100).unwrap();
        write(&mut buf, b"", 100).unwrap();
        let mut r = buf.as_slice();
        assert_eq!(read(&mut r, 100).unwrap().unwrap(), b"{\"t\":\"hello\"}");
        assert_eq!(read(&mut r, 100).unwrap().unwrap(), b"");
        assert!(read(&mut r, 100).unwrap().is_none());
        assert!(write(Vec::new(), &[0; 101], 100).is_err());
    }

    #[test]
    fn rejects_oversized_and_truncated() {
        let big = 1_000_000u32.to_le_bytes();
        assert!(read(&big[..], 100).is_err());
        let mut cut = Vec::new();
        write(&mut cut, b"abcdef", 100).unwrap();
        cut.truncate(7);
        assert!(read(&cut[..], 100).is_err());
    }
}
