//! "Iniciar com o sistema": arquivo `.desktop` em `~/.config/autostart` com `<app> --tray`.
//! "Start with the system": `.desktop` file in `~/.config/autostart` with `<app> --tray`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "sigilvault.desktop";

fn autostart_dir() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| io::Error::other("sem pasta de configuração / no config folder"))?;
    Ok(base.join("autostart"))
}

/// Aspas de uma linha `Exec` do `.desktop`. / Quoting for a `.desktop` `Exec` line.
fn exec_quote(path: &str) -> String {
    let mut out = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        // `%` é código de campo no `Exec`; `%%` é um `%` literal. / `%` is a field code in `Exec`; `%%` is a literal `%`.
        if c == '%' {
            out.push('%');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Conteúdo do arquivo `.desktop`. / Contents of the `.desktop` file.
pub fn entry_for(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=SigilVault\nExec={} --tray\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_quote(&exe.to_string_lossy())
    )
}

/// Liga ou desliga, apontando para `exe`. Dentro de um AppImage, aponta para o próprio AppImage.
/// Turns it on or off, pointing at `exe`. Inside an AppImage, points at the AppImage itself.
pub fn set(enabled: bool, exe: &Path) -> io::Result<()> {
    let dir = autostart_dir()?;
    let file = dir.join(FILE);
    if !enabled {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let target = std::env::var_os("APPIMAGE").map(PathBuf::from).unwrap_or_else(|| exe.to_path_buf());
    fs::create_dir_all(&dir)?;
    fs::write(file, entry_for(&target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_line_is_quoted_and_escaped() {
        let e = entry_for(Path::new("/opt/Sigil Vault/100%/app"));
        assert!(e.contains("Exec=\"/opt/Sigil Vault/100%%/app\" --tray\n"));
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert_eq!(exec_quote("a\"b$c"), "\"a\\\"b\\$c\"");
    }
}
