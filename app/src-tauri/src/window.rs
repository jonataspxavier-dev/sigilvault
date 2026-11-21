//! A janela do app: criada só quando aberta e destruída ao fechar ou bloquear, para liberar a
//! memória do WebView2 (e qualquer senha que estivesse na tela).
//! The app window: created only when opened and destroyed on close or lock, to free WebView2
//! memory (and any password that was on screen).

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Prefixo dos rótulos; as permissões valem para `main-*`.
/// Label prefix; permissions apply to `main-*`.
const PREFIX: &str = "main-";
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Argumentos do WebView2 (iguais em todas as janelas: o WebView2 recusa misturar). Mantém os
/// padrões do Tauri (sem SmartScreen, que consultaria a Microsoft) e corta conexões em segundo
/// plano e relatórios de falha, que poderiam levar memória da janela com uma senha visível.
/// WebView2 arguments (the same for every window: WebView2 refuses to mix). Keeps Tauri's defaults
/// (no SmartScreen, which would query Microsoft) and cuts background connections and crash
/// reports, which could carry window memory with a visible password.
pub(crate) const WEBVIEW_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
--disable-background-networking --disable-component-update --disable-domain-reliability --disable-sync --no-pings \
--disable-breakpad --disable-crash-reporter";

/// A janela aberta agora, se houver. / The currently open window, if any.
pub fn current(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows().into_iter().find(|(label, _)| label.starts_with(PREFIX)).map(|(_, w)| w)
}

/// Mostra a janela, criando uma nova se não existir. / Shows the window, creating a new one if needed.
pub fn open(app: &AppHandle) {
    if let Some(w) = current(app) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    create(app);
}

/// Cria sempre uma janela nova, com rótulo novo. Usado logo após destruir a anterior, que pode
/// ainda constar na lista enquanto fecha.
/// Always creates a new window with a new label. Used right after destroying the previous one,
/// which may still be listed while it closes.
pub fn create(app: &AppHandle) {
    let label = format!("{PREFIX}{}", COUNTER.fetch_add(1, Ordering::Relaxed) + 1);
    let _ = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("SigilVault")
        .inner_size(1040.0, 700.0)
        .min_inner_size(780.0, 540.0)
        .center()
        .additional_browser_args(WEBVIEW_ARGS)
        .build();
}

/// Destrói a janela (se houver). / Destroys the window (if any).
pub fn destroy(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with(PREFIX) {
            let _ = w.destroy();
        }
    }
}

/// Prefixo das janelas do pop-up rápido (permissões próprias, mais restritas).
/// Prefix of the quick popup windows (their own, narrower permissions).
const QUICK_PREFIX: &str = "quick-";

/// Abre o pop-up rápido (sempre por cima, no centro, com foco). Fecha um anterior.
/// Opens the quick popup (always on top, centered, focused). Closes a previous one.
pub fn open_quick(app: &AppHandle) {
    destroy_quick(app);
    let label = format!("{QUICK_PREFIX}{}", COUNTER.fetch_add(1, Ordering::Relaxed) + 1);
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App("quick.html".into()))
        .title("SigilVault")
        .inner_size(520.0, 640.0)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .focused(true)
        .additional_browser_args(WEBVIEW_ARGS)
        .build();
    #[cfg(debug_assertions)]
    eprintln!("[sigilvault] pop-up rápido: {:?}", built.as_ref().map(|w| w.label().to_owned()).map_err(|e| e.to_string()));
    if let Ok(w) = built {
        let _ = w.set_focus();
    }
}

/// Fecha o pop-up rápido (se houver). / Closes the quick popup (if any).
pub fn destroy_quick(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with(QUICK_PREFIX) {
            let _ = w.destroy();
        }
    }
}
