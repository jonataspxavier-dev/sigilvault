//! Ícone da bandeja: abre a janela, bloqueia e sai. Mostra cadeado fechado ou aberto.
//! Tray icon: opens the window, locks and quits. Shows a closed or open padlock.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{AppState, lock_app};
use crate::window;

const TRAY_ID: &str = "sigilvault";
const ICON_LOCKED: &[u8] = include_bytes!("../icons/tray-locked.png");
const ICON_UNLOCKED: &[u8] = include_bytes!("../icons/tray-unlocked.png");

fn labels(language: &str) -> [&'static str; 4] {
    match language {
        "en" => ["Open SigilVault", "Lock", "Quit", "SigilVault — locked"],
        _ => ["Abrir o SigilVault", "Bloquear", "Sair", "SigilVault — bloqueado"],
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let language = app.state::<AppState>().lock().settings.language.clone();
    let [open, lock, quit, _] = labels(&language);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", open, true, None::<&str>)?,
            &MenuItem::with_id(app, "lock", lock, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", quit, true, None::<&str>)?,
        ],
    )
}

/// Cria o ícone da bandeja. / Creates the tray icon.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_LOCKED)?)
        .tooltip(labels("pt")[3])
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => window::open(app),
            "lock" => lock_app(app, false),
            "quit" => {
                lock_app(app, false);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                window::open(tray.app_handle());
            }
        })
        .build(app)?;
    set_locked(app, true);
    Ok(())
}

/// Troca o ícone e a dica conforme o estado. / Switches icon and tooltip by state.
pub fn set_locked(app: &AppHandle, locked: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let bytes = if locked { ICON_LOCKED } else { ICON_UNLOCKED };
    if let Ok(img) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(img));
    }
    let language = app.state::<AppState>().lock().settings.language.clone();
    let tip = if locked { labels(&language)[3] } else { "SigilVault" };
    let _ = tray.set_tooltip(Some(tip));
}

/// Refaz o menu após trocar o idioma. / Rebuilds the menu after a language change.
pub fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
    let locked = app.state::<AppState>().lock().vault.is_none();
    set_locked(app, locked);
}
