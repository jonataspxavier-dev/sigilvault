//! A janela onde o Auto-Type vai digitar: capturada no instante do atalho, antes de o pop-up
//! do SigilVault roubar o foco. O programa é identificado pelo EXECUTÁVEL (caminho completo),
//! nunca pelo título da janela, que qualquer programa pode imitar.
//! The window where Auto-Type will type: captured at hotkey time, before the SigilVault popup
//! steals focus. The program is identified by its EXECUTABLE (full path), never by the window
//! title, which any program can imitate.

use std::path::PathBuf;

use sigilvault_ipc::process;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

/// Navegadores: lá quem preenche é a extensão (o planejamento proíbe o Auto-Type neles).
/// Browsers: the extension fills there (the plan forbids Auto-Type in them).
const BROWSERS: &[&str] = &[
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe", "iexplore.exe",
    "chromium.exe", "waterfox.exe", "librewolf.exe", "floorp.exe", "zen.exe", "arc.exe", "thorium.exe",
];

/// Janela-alvo. / Target window.
#[derive(Debug, Clone)]
pub struct Target {
    /// HWND como número (para atravessar threads). / HWND as a number (to cross threads).
    pub hwnd: usize,
    pub pid: u32,
    pub exe: PathBuf,
    /// Título, só para mostrar ao usuário. / Title, display only.
    pub title: String,
}

/// Janela em primeiro plano agora. / Current foreground window.
pub fn foreground() -> Option<Target> {
    // SAFETY: consultas simples sobre a janela em primeiro plano; o buffer tem o tamanho informado.
    // SAFETY: simple queries about the foreground window; the buffer has the given size.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        let title = String::from_utf16_lossy(&buf[..len]);
        let exe = process::image_path(pid).ok()?;
        Some(Target { hwnd: hwnd as usize, pid, exe, title })
    }
}

/// Nível de integridade (RID) de um processo; `None` se não der para consultar.
/// Integrity level (RID) of a process; `None` if it cannot be queried.
fn integrity(process: HANDLE) -> Option<u32> {
    // SAFETY: token aberto só para consulta e fechado; buffer alinhado do tamanho informado.
    // SAFETY: token opened for query only and closed; aligned buffer of the reported size.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TokenIntegrityLevel, std::ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let ok = GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr().cast(), needed, &mut needed);
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(label.Label.Sid, u32::from(count) - 1))
    }
}

impl Target {
    /// Nome do arquivo do executável (ex.: `notepad.exe`). / Executable file name.
    pub fn exe_name(&self) -> String {
        self.exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// É um navegador? / Is it a browser?
    pub fn is_browser(&self) -> bool {
        let name = self.exe_name().to_lowercase();
        BROWSERS.contains(&name.as_str())
    }

    /// É o próprio SigilVault? / Is it SigilVault itself?
    pub fn is_ours(&self) -> bool {
        self.pid == std::process::id()
    }

    /// Roda com mais privilégio que nós (administrador)? O Windows bloqueia digitar nela (UIPI).
    /// Se nem dá para consultar, trata como mais privilegiada.
    /// Runs with more privilege than us (administrator)? Windows blocks typing into it (UIPI).
    /// If it cannot even be queried, it is treated as more privileged.
    pub fn elevated(&self) -> bool {
        // SAFETY: handle aberto só para consulta e fechado. / Handle opened for query only and closed.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, self.pid);
            if process.is_null() {
                return true;
            }
            let theirs = integrity(process);
            CloseHandle(process);
            match (theirs, integrity(GetCurrentProcess())) {
                (Some(t), Some(o)) => t > o,
                _ => true,
            }
        }
    }

    /// A mesma janela (e o mesmo processo) continua em primeiro plano?
    /// Is the same window (and the same process) still in the foreground?
    pub fn still_foreground(&self) -> bool {
        // SAFETY: consultas simples. / Simple queries.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd as usize != self.hwnd {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd as HWND, &mut pid);
            pid == self.pid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(exe: &str) -> Target {
        Target { hwnd: 0, pid: 0, exe: PathBuf::from(exe), title: String::new() }
    }

    #[test]
    fn recognizes_browsers_by_executable() {
        assert!(fake(r"C:\Program Files\Google\Chrome\Application\chrome.exe").is_browser());
        assert!(fake(r"C:\x\Firefox.EXE").is_browser());
        assert!(!fake(r"C:\Windows\notepad.exe").is_browser());
        // Título não importa: "chrome.exe" no título de outro programa não engana.
        // Title does not matter: "chrome.exe" in another program's title does not fool it.
        let mut t = fake(r"C:\Windows\notepad.exe");
        t.title = "chrome.exe".into();
        assert!(!t.is_browser());
    }

    #[test]
    fn own_process_is_not_elevated_relative_to_itself() {
        let me = Target { hwnd: 0, pid: std::process::id(), exe: std::env::current_exe().unwrap(), title: String::new() };
        assert!(me.is_ours());
        assert!(!me.elevated());
        // Processo que não existe: tratado como privilegiado (não digita). / Nonexistent process: treated as privileged.
        assert!(Target { pid: u32::MAX - 3, ..me }.elevated());
    }
}
