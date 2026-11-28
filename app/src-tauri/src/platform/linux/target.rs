//! Programa em primeiro plano (alvo do Auto-Type). No Linux ele vem do último elemento que ganhou
//! o foco, avisado pela acessibilidade (AT-SPI); funciona igual no Wayland e no X11.
//! Foreground program (Auto-Type target). On Linux it comes from the last element that got focus,
//! reported by accessibility (AT-SPI); it works the same on Wayland and X11.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Navegadores: lá quem preenche é a extensão (o planejamento proíbe o Auto-Type neles).
/// Browsers: the extension fills there (the plan forbids Auto-Type in them).
const BROWSERS: &[&str] = &[
    "chrome", "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "msedge", "microsoft-edge",
    "microsoft-edge-stable", "firefox", "firefox-bin", "brave", "brave-browser", "opera", "vivaldi", "vivaldi-bin",
    "librewolf", "waterfox", "floorp", "zen", "zen-browser", "thorium-browser", "epiphany", "falkon",
];

/// Janela-alvo. / Target window.
#[derive(Debug, Clone)]
pub struct Target {
    /// Identificador da janela (X11) ou 0. / Window identifier (X11) or 0.
    pub hwnd: usize,
    pub pid: u32,
    pub exe: PathBuf,
    /// Título, só para mostrar ao usuário. / Title, display only.
    pub title: String,
}

/// O programa que tinha o foco por último (nunca o próprio SigilVault). Sem acessibilidade
/// ligada, ou antes do primeiro foco, não há resposta. / The program that had focus last (never
/// SigilVault itself). Without accessibility on, or before the first focus, there is no answer.
pub fn foreground() -> Option<Target> {
    super::atspi::start();
    super::atspi::last_focus().map(|f| Target { hwnd: usize::try_from(f.token).unwrap_or(0), pid: f.pid, exe: f.exe, title: f.app_name })
}

impl Target {
    /// Nome do arquivo do executável (ex.: `gedit`). / Executable file name.
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

    /// Roda como outro usuário? Aí não digitamos nela. Se nem dá para consultar, trata como outro.
    /// Runs as another user? Then we do not type into it. If it cannot even be queried, it counts as another.
    pub fn elevated(&self) -> bool {
        // SAFETY: `geteuid` não recebe argumentos e não falha. / `geteuid` takes no arguments and cannot fail.
        let me = unsafe { libc::geteuid() };
        std::fs::metadata(format!("/proc/{}", self.pid)).map(|m| m.uid() != me).unwrap_or(true)
    }

    /// O processo ainda existe e é o mesmo executável? / Does the process still exist and run the same executable?
    pub fn exists(&self) -> bool {
        std::fs::read_link(format!("/proc/{}/exe", self.pid)).is_ok_and(|p| p == self.exe)
    }

    /// O mesmo programa continua com o foco? / Does the same program still have focus?
    pub fn still_foreground(&self) -> bool {
        super::atspi::last_focus().is_some_and(|f| f.pid == self.pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(exe: &str, pid: u32) -> Target {
        Target { hwnd: 0, pid, exe: PathBuf::from(exe), title: String::new() }
    }

    #[test]
    fn recognizes_browsers() {
        assert!(target("/usr/bin/firefox", 1).is_browser());
        assert!(target("/opt/google/chrome/chrome", 1).is_browser());
        assert!(!target("/usr/bin/gedit", 1).is_browser());
    }

    #[test]
    fn own_process_is_ours_and_not_elevated() {
        let me = std::env::current_exe().unwrap();
        let t = target(me.to_str().unwrap(), std::process::id());
        assert!(t.is_ours());
        assert!(!t.elevated());
        assert!(t.exists());
        assert!(!target("/usr/bin/gedit", u32::MAX).exists());
    }
}
