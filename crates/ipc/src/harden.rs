//! Endurecimento do processo, chamado no começo do app e do conector.
//! Process hardening, called at the start of the app and the connector.

use windows_sys::Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, SetDefaultDllDirectories};

/// DLLs carregadas depois daqui só vêm da pasta do programa (protegida na instalação) e do
/// System32 — nunca da pasta atual nem do PATH, onde alguém poderia plantar uma DLL falsa.
/// DLLs loaded from here on only come from the program folder (protected in the install) and
/// System32 — never from the current folder or PATH, where someone could plant a fake DLL.
pub fn restrict_dll_search() -> bool {
    // SAFETY: só muda a ordem de busca de DLLs deste processo. / SAFETY: only changes this process's DLL search order.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) != 0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dll_search_can_be_restricted() {
        assert!(super::restrict_dll_search());
        // Uma DLL do sistema continua carregando. / A system DLL still loads.
        let name: Vec<u16> = "version.dll\0".encode_utf16().collect();
        // SAFETY: nome terminado em zero. / SAFETY: zero-terminated name.
        let h = unsafe { windows_sys::Win32::System::LibraryLoader::LoadLibraryW(name.as_ptr()) };
        assert!(!h.is_null());
    }
}
