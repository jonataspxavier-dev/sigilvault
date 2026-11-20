//! Inatividade do usuário e exclusão dos relatórios de erro do Windows.
//! User idle time and exclusion from Windows Error Reporting.

use windows_sys::Win32::System::ErrorReporting::WerAddExcludedApplication;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

use super::wide;

/// Milissegundos desde o último uso de teclado ou mouse no Windows. Usa o relógio monotônico
/// do sistema (GetTickCount), então mudar a hora do PC não engana o bloqueio.
/// Milliseconds since the last keyboard or mouse input in Windows. Uses the system monotonic
/// clock (GetTickCount), so changing the PC time does not fool auto-lock.
pub fn idle_millis() -> u64 {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` é válido e tem `cbSize` preenchido, como a API exige.
    // SAFETY: `info` is valid and has `cbSize` set, as the API requires.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0;
    }
    // SAFETY: função sem parâmetros. `wrapping_sub` trata a virada do contador a cada 49 dias.
    // SAFETY: parameterless function. `wrapping_sub` handles the counter wrap every 49 days.
    u64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime))
}

/// Exclui o executável dos relatórios de erro do Windows, que poderiam levar um dump de memória
/// com segredos. Vale só para o usuário atual.
/// Excludes the executable from Windows Error Reporting, which could ship a memory dump
/// containing secrets. Applies to the current user only.
pub fn exclude_from_error_reporting() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(name) = exe.file_name().and_then(|n| n.to_str()) else { return };
    // SAFETY: o nome é UTF-16 terminado em zero e vive durante a chamada.
    // SAFETY: the name is zero-terminated UTF-16 and lives during the call.
    unsafe { WerAddExcludedApplication(wide(name).as_ptr(), 0) };
}

/// Variáveis de ambiente que mudam o WebView2. Uma delas liga a porta de depuração remota, que
/// permitiria a outro programa comandar a interface do app. Na versão final, são apagadas.
/// Environment variables that change WebView2. One of them turns on the remote debugging port,
/// which would let another program drive the app's UI. In the release build, they are removed.
const WEBVIEW2_VARS: &[&str] = &[
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
];

/// Apaga as variáveis do WebView2 (só na versão final; em depuração os testes precisam delas).
/// Chamar no começo de `run`, antes de qualquer outra thread existir.
/// Removes the WebView2 variables (release build only; debug tests need them).
/// Call at the start of `run`, before any other thread exists.
pub fn harden_webview_env() {
    if cfg!(debug_assertions) {
        return;
    }
    for var in WEBVIEW2_VARS {
        // SAFETY: chamado no começo do programa, com uma única thread; ninguém lê o ambiente ao mesmo tempo.
        // SAFETY: called at program start, single-threaded; nobody reads the environment concurrently.
        unsafe { std::env::remove_var(var) };
    }
}
