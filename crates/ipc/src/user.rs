//! SID (identificador) do usuário do Windows atual. / Current Windows user's SID.

use std::io;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// SID do usuário atual em texto (ex.: `S-1-5-21-...`). / Current user's SID as text.
pub fn current_sid() -> io::Result<String> {
    // SAFETY: chamadas Win32 com buffers do tamanho informado; tudo é liberado no fim.
    // SAFETY: Win32 calls with buffers of the reported size; everything is freed at the end.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        // u64 para alinhar a estrutura. / u64 to align the structure.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), needed, &mut needed);
        CloseHandle(token);
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(sid)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sid_looks_right() {
        let sid = super::current_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert!(sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
