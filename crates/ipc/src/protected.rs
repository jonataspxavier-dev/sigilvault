//! "Instalação protegida": sem certificado de assinatura, a confiança entre o app e o host vem da
//! pasta onde estão. Se nem o usuário atual (sem privilégios de administrador) consegue alterar a
//! pasta nem os executáveis, um programa malicioso rodando como ele também não consegue trocá-los.
//! É o caso de uma instalação em Program Files.
//! "Protected install": without a signing certificate, trust between the app and the host comes
//! from the folder they live in. If not even the current user (without administrator rights) can
//! change the folder or the executables, a malicious program running as that user cannot swap
//! them either. That is the case for an install in Program Files.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AccessCheck, DACL_SECURITY_INFORMATION, DuplicateToken, GENERIC_MAPPING, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PRIVILEGE_SET, PSECURITY_DESCRIPTOR, SecurityImpersonation, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ALL_ACCESS, FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_WRITE_DATA, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Direitos que permitiriam trocar ou plantar arquivos. / Rights that would allow swapping or planting files.
const DIR_WRITE: [u32; 6] = [FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_DELETE_CHILD, DELETE, WRITE_DAC, WRITE_OWNER];
const FILE_WRITE: [u32; 4] = [FILE_WRITE_DATA, DELETE, WRITE_DAC, WRITE_OWNER];

/// O usuário atual (este processo) tem ALGUM dos direitos sobre o caminho? Na dúvida (erro ao
/// consultar), responde `true` — tratar como alterável é o lado seguro.
/// Does the current user (this process) have ANY of the rights over the path? When in doubt
/// (query error), answers `true` — treating it as changeable is the safe side.
fn any_right(path: &Path, rights: &[u32]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: descritor e tokens obtidos do Windows, usados durante a função e liberados no fim.
    // SAFETY: descriptor and tokens obtained from Windows, used during the function and freed at the end.
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let info = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        if GetNamedSecurityInfoW(wide.as_ptr(), SE_FILE_OBJECT, info, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &mut sd) != 0 {
            return true;
        }
        let mut token: HANDLE = std::ptr::null_mut();
        let mut imp: HANDLE = std::ptr::null_mut();
        let ok_token = OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut token) != 0
            && DuplicateToken(token, SecurityImpersonation, &mut imp) != 0;
        let mapping = GENERIC_MAPPING { GenericRead: FILE_GENERIC_READ, GenericWrite: FILE_GENERIC_WRITE, GenericExecute: FILE_GENERIC_EXECUTE, GenericAll: FILE_ALL_ACCESS };
        let mut result = !ok_token;
        if ok_token {
            for &right in rights {
                let mut privileges: PRIVILEGE_SET = std::mem::zeroed();
                let mut len = size_of::<PRIVILEGE_SET>() as u32;
                let mut granted = 0u32;
                let mut status = 0i32;
                let ok = AccessCheck(sd, imp, right, &mapping, &mut privileges, &mut len, &mut granted, &mut status);
                if ok == 0 || status != 0 {
                    // Erro na consulta ou direito concedido: alterável. / Query error or right granted: changeable.
                    result = true;
                    break;
                }
            }
        }
        if !imp.is_null() {
            CloseHandle(imp);
        }
        if !token.is_null() {
            CloseHandle(token);
        }
        LocalFree(sd);
        result
    }
}

/// A pasta (e a pasta-mãe, que poderia renomeá-la) está protegida contra o usuário atual?
/// Is the folder (and its parent, which could rename it) protected from the current user?
pub fn folder_protected(dir: &Path) -> bool {
    if any_right(dir, &DIR_WRITE) {
        return false;
    }
    match dir.parent() {
        Some(parent) => !any_right(parent, &[FILE_DELETE_CHILD]),
        None => true,
    }
}

/// O arquivo não pode ser alterado pelo usuário atual? / The file cannot be changed by the current user?
pub fn file_protected(file: &Path) -> bool {
    !any_right(file, &FILE_WRITE)
}

/// Instalação protegida: os dois executáveis na MESMA pasta, e pasta e arquivos fora do alcance do
/// usuário atual. / Protected install: both executables in the SAME folder, and the folder and
/// files out of the current user's reach.
pub fn protected_install(own: &Path, peer: &Path) -> bool {
    let (Ok(own), Ok(peer)) = (std::fs::canonicalize(own), std::fs::canonicalize(peer)) else { return false };
    let (Some(dir), Some(peer_dir)) = (own.parent(), peer.parent()) else { return false };
    let same = dir.as_os_str().to_string_lossy().to_lowercase() == peer_dir.as_os_str().to_string_lossy().to_lowercase();
    same && folder_protected(dir) && file_protected(&own) && file_protected(&peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_folder_is_not_protected() {
        let dir = std::env::temp_dir().join(format!("sv-prot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.exe");
        std::fs::write(&file, b"x").unwrap();
        assert!(!folder_protected(&dir));
        assert!(!file_protected(&file));
        assert!(!protected_install(&file, &file));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_folder_is_protected_for_a_normal_process() {
        let windir = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let notepad = windir.join("System32").join("notepad.exe");
        // Só vale rodando sem privilégio de administrador (o caso normal). / Only valid when not elevated (the normal case).
        if any_right(&windir.join("System32"), &[FILE_ADD_FILE]) {
            eprintln!("processo com privilégio de administrador: teste pulado / elevated process: test skipped");
            return;
        }
        assert!(folder_protected(&windir.join("System32")));
        assert!(file_protected(&notepad));
        // Mesmo protegidos, precisam estar na mesma pasta. / Even protected, they must be in the same folder.
        assert!(protected_install(&notepad, &windir.join("System32").join("cmd.exe")));
        assert!(!protected_install(&notepad, &windir.join("explorer.exe")));
    }

    #[test]
    fn missing_path_counts_as_changeable() {
        assert!(!folder_protected(Path::new(r"C:\nao\existe")));
        assert!(!file_protected(Path::new(r"C:\nao\existe.exe")));
    }
}
