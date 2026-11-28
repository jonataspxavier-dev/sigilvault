//! "Iniciar com o Windows": valor `SigilVault` em HKCU\...\Run com `"<app>" --tray`. O app abre só
//! na bandeja e o cofre continua bloqueado até a senha mestra. O desinstalador apaga o valor.
//! "Start with Windows": `SigilVault` value in HKCU\...\Run with `"<app>" --tray`. The app opens in
//! the tray only and the vault stays locked until the master password. The uninstaller deletes it.

use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
#[cfg(test)]
use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};

use super::wide;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "SigilVault";

/// Linha de comando gravada no registro. / Command line written to the registry.
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\" --tray", exe.display())
}

/// Liga ou desliga, apontando para `exe`. / Turns it on or off, pointing at `exe`.
pub fn set(enabled: bool, exe: &Path) -> std::io::Result<()> {
    set_value(RUN_KEY, VALUE, enabled.then(|| command_for(exe)).as_deref())
}

fn set_value(key: &str, name: &str, data: Option<&str>) -> std::io::Result<()> {
    let (key, name) = (wide(key), wide(name));
    // SAFETY: textos UTF-16 terminados em zero que vivem durante a chamada; tamanho em bytes inclui o zero.
    // SAFETY: zero-terminated UTF-16 strings alive during the call; byte size includes the terminator.
    let status = unsafe {
        match data {
            Some(data) => {
                let data = wide(data);
                RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
            }
            None => match RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) {
                // Já não existia: nada a fazer. / Did not exist: nothing to do.
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                other => other,
            },
        }
    };
    if status == ERROR_SUCCESS { Ok(()) } else { Err(std::io::Error::from_raw_os_error(status as i32)) }
}

#[cfg(test)]
fn get_value(key: &str, name: &str) -> Option<String> {
    let (key, name) = (wide(key), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: buffer de `size` bytes, válido durante a chamada. / SAFETY: buffer of `size` bytes, valid during the call.
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_quoted() {
        assert_eq!(command_for(Path::new(r"C:\Program Files\SigilVault\sigilvault-app.exe")), r#""C:\Program Files\SigilVault\sigilvault-app.exe" --tray"#);
    }

    #[test]
    fn set_read_delete_roundtrip() {
        // Usa uma chave de teste própria, não a Run real. / Uses a test key of its own, not the real Run key.
        let key = r"Software\SigilVaultTest";
        let name = format!("startup-{}", std::process::id());
        set_value(key, &name, Some(r#""C:\x y\a.exe" --tray"#)).unwrap();
        assert_eq!(get_value(key, &name).as_deref(), Some(r#""C:\x y\a.exe" --tray"#));
        set_value(key, &name, None).unwrap();
        assert_eq!(get_value(key, &name), None);
        // Apagar de novo não é erro. / Deleting again is not an error.
        set_value(key, &name, None).unwrap();
        // SAFETY: texto terminado em zero. / SAFETY: zero-terminated string.
        unsafe { windows_sys::Win32::System::Registry::RegDeleteKeyW(HKEY_CURRENT_USER, wide(key).as_ptr()) };
    }
}
