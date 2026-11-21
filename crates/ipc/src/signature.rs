//! Assinatura digital (Authenticode) dos executáveis.
//! Digital signature (Authenticode) of the executables.
//!
//! Regra / Rule ([`peer_allowed`]):
//! - os dois assinados, com assinatura válida e o MESMO certificado → aceito;
//!   both signed, valid signature and the SAME certificate → accepted;
//! - os dois sem assinatura → aceito em builds de depuração, ou numa INSTALAÇÃO PROTEGIDA (mesma
//!   pasta, que o usuário atual não consegue alterar — ex.: Program Files). Sem certificado por
//!   enquanto, é o que protege a versão instalada; both unsigned → accepted in debug builds, or in
//!   a PROTECTED INSTALL (same folder, which the current user cannot change — e.g. Program
//!   Files). With no certificate for now, that is what protects the installed version;
//! - qualquer outra combinação → recusado. / any other combination → refused.

use std::path::Path;

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
    WTHelperProvDataFromStateData, WinVerifyTrust,
};

// Códigos de "não assinado". / "Unsigned" codes.
const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100_u32 as i32;
const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B_0003_u32 as i32;
const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B_0001_u32 as i32;

/// Situação da assinatura de um arquivo. / Signature status of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signer {
    /// Sem assinatura. / Unsigned.
    Unsigned,
    /// Assinatura válida; SHA-256 do certificado de quem assinou. / Valid signature; SHA-256 of the signer's certificate.
    Trusted([u8; 32]),
    /// Assinatura inválida, adulterada ou não confiável (ou arquivo ilegível).
    /// Invalid, tampered or untrusted signature (or unreadable file).
    Untrusted,
}

/// Confere a assinatura de um arquivo (sem consultar a internet). / Checks a file's signature (without going online).
pub fn signer(path: &Path) -> Signer {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: estruturas inicializadas e vivas durante as chamadas; o estado é fechado no fim.
    // SAFETY: structures initialized and alive during the calls; the state is closed at the end.
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: wide.as_ptr(),
            hFile: std::ptr::null_mut(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut data: WINTRUST_DATA = std::mem::zeroed();
        data.cbStruct = size_of::<WINTRUST_DATA>() as u32;
        data.dwUIChoice = WTD_UI_NONE;
        data.fdwRevocationChecks = WTD_REVOKE_NONE;
        data.dwUnionChoice = WTD_CHOICE_FILE;
        data.Anonymous.pFile = &mut file;
        data.dwStateAction = WTD_STATEACTION_VERIFY;
        data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let hwnd = INVALID_HANDLE_VALUE; // sem interface / no UI
        let status = WinVerifyTrust(hwnd, &mut action, (&mut data as *mut WINTRUST_DATA).cast());
        let result = match status {
            0 => leaf_certificate_hash(&data).map_or(Signer::Untrusted, Signer::Trusted),
            TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => Signer::Unsigned,
            _ => Signer::Untrusted,
        };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(hwnd, &mut action, (&mut data as *mut WINTRUST_DATA).cast());
        result
    }
}

/// SHA-256 do certificado de quem assinou. / SHA-256 of the signer's certificate.
///
/// # Safety
/// `data` precisa vir de uma verificação bem-sucedida ainda aberta.
/// `data` must come from a successful verification that is still open.
unsafe fn leaf_certificate_hash(data: &WINTRUST_DATA) -> Option<[u8; 32]> {
    unsafe {
        let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return None;
        }
        let cert = (*(*signer).pasCertChain).pCert;
        if cert.is_null() || (*cert).pbCertEncoded.is_null() {
            return None;
        }
        let der = std::slice::from_raw_parts((*cert).pbCertEncoded, (*cert).cbCertEncoded as usize);
        Some(Sha256::digest(der).into())
    }
}

/// O executável do outro lado pode conversar com o nosso? (Regra no topo do arquivo.)
/// May the executable on the other side talk to ours? (Rule at the top of the file.)
pub fn peer_allowed(own: &Path, peer: &Path) -> bool {
    match (signer(own), signer(peer)) {
        (Signer::Trusted(a), Signer::Trusted(b)) => a == b,
        (Signer::Unsigned, Signer::Unsigned) => cfg!(debug_assertions) || crate::protected::protected_install(own, peer),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_is_unsigned() {
        let me = std::env::current_exe().unwrap();
        assert_eq!(signer(&me), Signer::Unsigned);
        // Em depuração, dois executáveis sem assinatura conversam; em release, não.
        // In debug, two unsigned executables may talk; in release, not.
        assert_eq!(peer_allowed(&me, &me), cfg!(debug_assertions));
    }

    #[test]
    fn missing_file_is_not_trusted() {
        assert_eq!(signer(Path::new(r"C:\nao\existe.exe")), Signer::Untrusted);
        assert!(!peer_allowed(&std::env::current_exe().unwrap(), Path::new(r"C:\nao\existe.exe")));
    }

    #[test]
    fn tampered_signed_file_is_untrusted() {
        // Copia um executável assinado conhecido e altera 1 byte: a assinatura deixa de valer.
        // Copies a known signed executable and changes 1 byte: the signature stops being valid.
        let candidates = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            r"C:\Program Files\Git\cmd\git.exe",
        ];
        let Some(src) = candidates.iter().map(Path::new).find(|p| matches!(signer(p), Signer::Trusted(_))) else {
            eprintln!("nenhum executável assinado encontrado; teste pulado / no signed executable found; test skipped");
            return;
        };
        let Signer::Trusted(hash) = signer(src) else { unreachable!() };
        assert_eq!(signer(src), Signer::Trusted(hash), "resultado estável / stable result");
        let dir = std::env::temp_dir().join(format!("sv-sig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let copy = dir.join("copia.exe");
        let mut bytes = std::fs::read(src).unwrap();
        assert_eq!(std::fs::write(&copy, &bytes).ok(), Some(()));
        assert_eq!(signer(&copy), Signer::Trusted(hash), "cópia intacta continua válida / intact copy stays valid");
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        std::fs::write(&copy, &bytes).unwrap();
        assert_eq!(signer(&copy), Signer::Untrusted);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
