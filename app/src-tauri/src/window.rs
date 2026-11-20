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

fn current(app: &AppHandle) -> Option<WebviewWindow> {
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
