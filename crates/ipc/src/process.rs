//! Caminho do executável de um processo e comparação de arquivos.
//! Executable path of a process and file comparison.

use std::io;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};

/// Caminho completo do executável do processo `pid`. / Full executable path of process `pid`.
pub fn image_path(pid: u32) -> io::Result<PathBuf> {
    // SAFETY: handle aberto só para consulta e fechado no fim; buffer do tamanho informado.
    // SAFETY: handle opened for query only and closed at the end; buffer of the given size.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u16; 32 * 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        use std::os::windows::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
    }
}

/// Os dois caminhos são o mesmo arquivo? Resolve atalhos, nomes curtos (8.3) e maiúsculas.
/// Are both paths the same file? Resolves links, short (8.3) names and case.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase(),
        _ => false,
    }
}

/// Arquivo com esse nome na mesma pasta do executável atual. / File with this name in the current executable's folder.
pub fn sibling(name: &str) -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| io::Error::other("executável sem pasta / executable without folder"))?;
    Ok(dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_image_and_compares() {
        let me = image_path(std::process::id()).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert!(same_file(&me, &exe));
        let upper = PathBuf::from(exe.to_string_lossy().to_uppercase());
        assert!(same_file(&upper, &exe));
        assert!(!same_file(&exe, Path::new(r"C:\Windows\System32\notepad.exe")));
        assert!(!same_file(&exe, Path::new(r"C:\nao\existe.exe")));
        assert!(image_path(u32::MAX - 3).is_err());
    }
}
