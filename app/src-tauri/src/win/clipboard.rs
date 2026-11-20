//! Área de transferência protegida: a senha copiada não entra no histórico (Win+V) nem na nuvem,
//! e é apagada depois de alguns segundos se ainda estiver lá.
//! Protected clipboard: the copied password stays out of history (Win+V) and the cloud,
//! and is wiped after a few seconds if it is still there.

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::wide;

/// Fecha a área de transferência ao sair de escopo, mesmo em caso de erro.
/// Closes the clipboard when dropped, even on error.
struct OpenGuard;

impl OpenGuard {
    fn open(owner: HWND) -> Option<Self> {
        // SAFETY: OpenClipboard só recebe um HWND (pode ser de outra thread); falha retorna 0.
        // SAFETY: OpenClipboard only takes an HWND (may belong to another thread); failure returns 0.
        (unsafe { OpenClipboard(owner) } != 0).then_some(Self)
    }
}

impl Drop for OpenGuard {
    fn drop(&mut self) {
        // SAFETY: só existe um OpenGuard quando OpenClipboard teve sucesso.
        // SAFETY: an OpenGuard only exists when OpenClipboard succeeded.
        unsafe { CloseClipboard() };
    }
}

/// Copia `bytes` para um bloco de memória global e entrega à área de transferência.
/// Copies `bytes` into a global memory block and hands it to the clipboard.
fn put(format: u32, bytes: &[u8]) -> Result<(), ()> {
    // SAFETY: alocamos `len` bytes, copiamos exatamente `len` bytes enquanto o bloco está travado,
    // e só liberamos o bloco se o Windows não ficou com ele (SetClipboardData falhou).
    // SAFETY: we allocate `len` bytes, copy exactly `len` bytes while the block is locked,
    // and only free it if Windows did not take ownership (SetClipboardData failed).
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if mem.is_null() {
            return Err(());
        }
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            GlobalFree(mem);
            return Err(());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        GlobalUnlock(mem);
        if SetClipboardData(format, mem as HANDLE).is_null() {
            GlobalFree(mem);
            return Err(());
        }
    }
    Ok(())
}

fn register(name: &str) -> u32 {
    // SAFETY: o nome é UTF-16 terminado em zero e vive até o fim da chamada.
    // SAFETY: the name is zero-terminated UTF-16 and lives until the call returns.
    unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
}

/// Marca de uma cópia: número de sequência e impressão digital (SHA-256 com sal) do texto.
/// Nunca guardamos a senha em si, só a impressão, para reconhecê-la na hora de limpar.
/// Mark of a copy: sequence number and fingerprint (salted SHA-256) of the text.
/// We never keep the password itself, only the fingerprint, to recognize it when clearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    sequence: u32,
    salt: [u8; 16],
    digest: [u8; 32],
}

fn fingerprint(salt: &[u8; 16], utf16_le: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"sigilvault clipboard v1");
    hasher.update(salt);
    hasher.update(utf16_le);
    hasher.finalize().into()
}

/// Copia um segredo com os formatos que pedem ao Windows para não guardar no histórico nem
/// sincronizar. Devolve a marca da cópia, usada para limpar depois.
/// Copies a secret with the formats that ask Windows not to keep it in history nor sync it.
/// Returns the copy mark, used for clearing later.
pub fn set_secret_text(owner: HWND, text: &str) -> Result<Copied, ()> {
    let utf16: Zeroizing<Vec<u16>> = Zeroizing::new(wide(text));
    let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(utf16.iter().flat_map(|c| c.to_le_bytes()).collect());
    let salt: [u8; 16] = sigilvault_core::crypto::random_array().map_err(|_| ())?;
    // Impressão sem o zero final, igual à leitura em `clear_if_ours`.
    // Fingerprint without the trailing zero, matching the read in `clear_if_ours`.
    let digest = fingerprint(&salt, &bytes[..bytes.len() - 2]);
    {
        let _guard = OpenGuard::open(owner).ok_or(())?;
        // SAFETY: a área de transferência está aberta por nós (guard acima).
        // SAFETY: the clipboard is open by us (guard above).
        if unsafe { EmptyClipboard() } == 0 {
            return Err(());
        }
        put(CF_UNICODETEXT as u32, &bytes)?;
        let zero = 0u32.to_le_bytes();
        put(register("ExcludeClipboardContentFromMonitorProcessing"), &zero)?;
        put(register("CanIncludeInClipboardHistory"), &zero)?;
        put(register("CanUploadToCloudClipboard"), &zero)?;
    }
    // SAFETY: função sem parâmetros. / SAFETY: parameterless function.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    Ok(Copied { sequence, salt, digest })
}

/// Apaga a área de transferência só se ela ainda tiver a nossa senha: mesma sequência ou mesmo
/// conteúdo (alguns programas regravam o texto e mudam a sequência). Antes de esvaziar,
/// sobrescreve a cópia com zeros.
/// Clears the clipboard only if it still holds our password: same sequence or same content
/// (some programs rewrite the text and change the sequence). Before emptying, overwrites the
/// copy with zeros.
pub fn clear_if_ours(owner: HWND, copied: &Copied) -> bool {
    let Some(_guard) = OpenGuard::open(owner) else { return false };
    // SAFETY: a área de transferência está aberta; o bloco devolvido é travado, lido e zerado
    // dentro do tamanho informado por GlobalSize, e destravado antes de esvaziar.
    // SAFETY: the clipboard is open; the returned block is locked, read and zeroed within the
    // size reported by GlobalSize, and unlocked before emptying.
    unsafe {
        let same_sequence = GetClipboardSequenceNumber() == copied.sequence;
        let mem = GetClipboardData(CF_UNICODETEXT as u32);
        if mem.is_null() {
            return false;
        }
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            return false;
        }
        let size = GlobalSize(mem);
        let data = std::slice::from_raw_parts(ptr, size);
        // Texto até o primeiro zero UTF-16. / Text up to the first UTF-16 zero.
        let len = data.as_chunks::<2>().0.iter().position(|c| *c == [0, 0]).map_or(size - size % 2, |i| i * 2);
        let ours = same_sequence || fingerprint(&copied.salt, &data[..len]) == copied.digest;
        if ours {
            std::ptr::write_bytes(ptr, 0, size);
        }
        GlobalUnlock(mem);
        ours && EmptyClipboard() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usa a área de transferência real: rode com `cargo test -- --ignored`.
    /// Uses the real clipboard: run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "sobrescreve a área de transferência / overwrites the clipboard"]
    fn copy_then_clear_even_if_rewritten() {
        crate::win::watcher::start(|| {});
        while crate::win::watcher::hwnd().is_null() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let owner = crate::win::watcher::hwnd();

        // Caso simples: ninguém mexeu. / Simple case: nobody touched it.
        let copied = set_secret_text(owner, "senha-de-teste-1").unwrap();
        assert!(clear_if_ours(owner, &copied));

        // Outro programa regravou o mesmo texto (sequência mudou): ainda limpa.
        // Another program rewrote the same text (sequence changed): still clears.
        let copied = set_secret_text(owner, "senha-de-teste-2").unwrap();
        set_secret_text(owner, "senha-de-teste-2").unwrap();
        assert!(clear_if_ours(owner, &copied));

        // O usuário copiou outra coisa: não pode apagar.
        // The user copied something else: must not clear.
        let copied = set_secret_text(owner, "senha-de-teste-3").unwrap();
        set_secret_text(owner, "texto do usuário").unwrap();
        assert!(!clear_if_ours(owner, &copied));
    }
}
