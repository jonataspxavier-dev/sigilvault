//! Janela oculta que recebe avisos do Windows: sessão bloqueada, desconectada, logoff,
//! suspensão e tela apagada (Modern Standby). Qualquer um deles bloqueia o cofre.
//! Hidden window that receives Windows notices: session locked, disconnected, logoff,
//! suspend and display off (Modern Standby). Any of them locks the vault.
//!
//! O HWND dessa janela também é o "dono" da área de transferência.
//! This window's HWND is also the clipboard "owner".

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    POWERBROADCAST_SETTING, RegisterPowerSettingNotification, RegisterSuspendResumeNotification,
};
use windows_sys::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
    PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, RegisterClassW, TranslateMessage, WM_POWERBROADCAST,
    WM_WTSSESSION_CHANGE, WNDCLASSW, WS_OVERLAPPED, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT,
    WTS_SESSION_LOCK, WTS_SESSION_LOGOFF,
};

use super::wide;

type Handler = Box<dyn Fn() + Send + Sync>;

static ON_LOCK: OnceLock<Handler> = OnceLock::new();
static HWND_ADDR: AtomicUsize = AtomicUsize::new(0);

/// HWND da janela oculta (nulo até ela ser criada).
/// Hidden window HWND (null until created).
pub fn hwnd() -> HWND {
    HWND_ADDR.load(Ordering::Acquire) as HWND
}

/// O evento de sessão ou energia deve bloquear o cofre?
/// Should the session or power event lock the vault?
pub fn should_lock(message: u32, wparam: usize, display_off: bool) -> bool {
    match message {
        WM_WTSSESSION_CHANGE => matches!(
            wparam as u32,
            WTS_SESSION_LOCK | WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT | WTS_SESSION_LOGOFF
        ),
        WM_POWERBROADCAST => wparam as u32 == PBT_APMSUSPEND || (wparam as u32 == PBT_POWERSETTINGCHANGE && display_off),
        _ => false,
    }
}

fn same_guid(a: &windows_sys::core::GUID, b: &windows_sys::core::GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let mut display_off = false;
    if msg == WM_POWERBROADCAST && wparam as u32 == PBT_POWERSETTINGCHANGE && lparam != 0 {
        // SAFETY: para PBT_POWERSETTINGCHANGE o Windows passa um POWERBROADCAST_SETTING válido em lparam.
        // SAFETY: for PBT_POWERSETTINGCHANGE Windows passes a valid POWERBROADCAST_SETTING in lparam.
        let setting = unsafe { &*(lparam as *const POWERBROADCAST_SETTING) };
        // Data = 0 significa tela desligada. / Data = 0 means display off.
        display_off = same_guid(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE) && setting.Data[0] == 0;
    }
    if should_lock(msg, wparam, display_off)
        && let Some(handler) = ON_LOCK.get()
    {
        handler();
    }
    if msg == WM_POWERBROADCAST {
        return 1;
    }
    // SAFETY: repassa a mensagem original para o tratamento padrão.
    // SAFETY: forwards the original message to default handling.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Cria a janela oculta numa thread própria e chama `on_lock` a cada evento de bloqueio.
/// Creates the hidden window on its own thread and calls `on_lock` on every lock event.
pub fn start(on_lock: impl Fn() + Send + Sync + 'static) {
    if ON_LOCK.set(Box::new(on_lock)).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("sigilvault-watcher".into())
        .spawn(|| {
            let class = wide("SigilVaultWatcher");
            let title = wide("SigilVault");
            // SAFETY: as strings vivem até o fim da thread; a classe e a janela são criadas nesta
            // thread, que fica bombeando mensagens até o processo terminar.
            // SAFETY: the strings live until the thread ends; the class and window are created on
            // this thread, which keeps pumping messages until the process exits.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                };
                RegisterClassW(&wc);
                // Janela de nível superior, nunca exibida: só elas recebem os avisos de energia.
                // Top-level window, never shown: only those receive power notices.
                let hwnd = CreateWindowExW(
                    0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0,
                    std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null(),
                );
                if hwnd.is_null() {
                    return;
                }
                HWND_ADDR.store(hwnd as usize, Ordering::Release);
                WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
                RegisterSuspendResumeNotification(hwnd as HANDLE, DEVICE_NOTIFY_WINDOW_HANDLE);
                RegisterPowerSettingNotification(hwnd as HANDLE, &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);

                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("thread do watcher / watcher thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PBT_APMRESUMEAUTOMATIC, WTS_SESSION_UNLOCK};

    #[test]
    fn lock_events() {
        for e in [WTS_SESSION_LOCK, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT, WTS_SESSION_LOGOFF] {
            assert!(should_lock(WM_WTSSESSION_CHANGE, e as usize, false));
        }
        assert!(!should_lock(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize, false));
        assert!(should_lock(WM_POWERBROADCAST, PBT_APMSUSPEND as usize, false));
        assert!(!should_lock(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, false));
        assert!(should_lock(WM_POWERBROADCAST, PBT_POWERSETTINGCHANGE as usize, true));
        assert!(!should_lock(WM_POWERBROADCAST, PBT_POWERSETTINGCHANGE as usize, false));
    }
}
