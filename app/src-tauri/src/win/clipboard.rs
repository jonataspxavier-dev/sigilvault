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

/// Copia um segredo com os formatos que pedem ao Windows para não guardar no histórico nem
/// sincronizar. Devolve o número de sequência para limpar depois.
/// Copies a secret with the formats that ask Windows not to keep it in history nor sync it.
/// Returns the sequence number for clearing later.
pub fn set_secret_text(owner: HWND, text: &str) -> Result<u32, ()> {
    let utf16: Zeroizing<Vec<u16>> = Zeroizing::new(wide(text));
    let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(utf16.iter().flat_map(|c| c.to_le_bytes()).collect());
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
    Ok(unsafe { GetClipboardSequenceNumber() })
}

/// Apaga a área de transferência só se o conteúdo ainda for o que copiamos (mesma sequência).
/// Antes de esvaziar, sobrescreve a cópia da senha com zeros.
/// Clears the clipboard only if it still holds what we copied (same sequence).
/// Before emptying, overwrites the password copy with zeros.
pub fn clear_if_unchanged(owner: HWND, sequence: u32) -> bool {
    // SAFETY: função sem parâmetros. / SAFETY: parameterless function.
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        return false;
    }
    let Some(_guard) = OpenGuard::open(owner) else { return false };
    // SAFETY: a área de transferência está aberta; o bloco devolvido é travado, zerado no tamanho
    // informado por GlobalSize e destravado antes de esvaziar.
    // SAFETY: the clipboard is open; the returned block is locked, zeroed for the size reported
    // by GlobalSize and unlocked before emptying.
    unsafe {
        let mem = GetClipboardData(CF_UNICODETEXT as u32);
        if !mem.is_null() {
            let ptr = GlobalLock(mem) as *mut u8;
            if !ptr.is_null() {
                std::ptr::write_bytes(ptr, 0, GlobalSize(mem));
                GlobalUnlock(mem);
            }
        }
        EmptyClipboard() != 0
    }
}
