//! App desktop do SigilVault: serviço da bandeja + janela Tauri.
//! SigilVault desktop app: tray service + Tauri window.

mod attempts;
mod backups;
mod bridge;
mod commands;
mod desktop;
mod dialogs;
mod rollback;
mod settings;
mod state;
mod storage;
mod transfer;
mod tray;
mod win;
mod window;

use std::time::Duration;

use tauri::{Manager, RunEvent};

use crate::state::{AppState, lock_app};

/// Intervalo de checagem da inatividade. / Idle check interval.
const IDLE_POLL: Duration = Duration::from_secs(5);

/// Liga/desliga "Iniciar com o Windows" apontando para este executável.
/// Turns "Start with Windows" on/off pointing at this executable.
pub(crate) fn apply_startup(enabled: bool) -> std::io::Result<()> {
    win::startup::set(enabled, &std::env::current_exe()?)
}

/// Inicia o app. / Starts the app.
pub fn run() {
    // Versão final: ninguém liga a depuração do WebView2 por variável de ambiente.
    // Release build: nobody turns on WebView2 debugging through an environment variable.
    win::system::harden_webview_env();
    // Sem relatórios de erro com dump de memória. / No error reports with memory dumps.
    win::system::exclude_from_error_reporting();

    let app = tauri::Builder::default()
        // Uma só instância: abrir de novo apenas mostra a janela.
        // Single instance: launching again just shows the window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::open(app)))
        // Diálogos nativos: usados só pelo Rust (a interface não tem permissão para eles).
        // Native dialogs: used only by Rust (the UI has no permission for them).
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            // Só em builds de depuração: pasta de dados alternativa, para testes não tocarem no cofre real.
            // Debug builds only: alternative data folder, so tests never touch the real vault.
            #[cfg(debug_assertions)]
            let dir = std::env::var_os("SIGILVAULT_DATA_DIR").map(std::path::PathBuf::from).unwrap_or(dir);
            let paths = storage::Paths::new(&dir)?;
            app.manage(bridge::Bridge::new(&paths));
            app.manage(AppState::new(paths));
            tray::create(app.handle())?;

            // Ponte com a extensão do navegador (pipe protegido). / Bridge with the browser extension (protected pipe).
            bridge::start(app.handle());

            // Atalhos globais e Auto-Type. / Global hotkeys and Auto-Type.
            app.manage(desktop::Desktop::default());
            desktop::start(app.handle());

            // Bloqueio por eventos do Windows. / Lock on Windows events.
            let handle = app.handle().clone();
            win::watcher::start(move || lock_app(&handle, false));

            // Bloqueio por inatividade. / Lock on inactivity.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(IDLE_POLL);
                let state = handle.state::<AppState>();
                let (unlocked, minutes) = {
                    let inner = state.lock();
                    (inner.vault.is_some(), inner.settings.auto_lock_minutes)
                };
                if unlocked && win::system::idle_millis() >= u64::from(minutes) * 60_000 {
                    lock_app(&handle, false);
                }
            });

            // "Iniciar com o Windows": regrava o caminho (o app pode ter sido reinstalado em outra
            // pasta). Só na versão final: a de desenvolvimento não deve tomar o lugar da instalada.
            // "Start with Windows": rewrites the path (the app may have been reinstalled elsewhere).
            // Release only: the development build must not take the installed one's place.
            if !cfg!(debug_assertions) {
                let start = app.state::<AppState>().lock().settings.start_with_windows;
                if let Err(e) = apply_startup(start) {
                    eprintln!("iniciar com o Windows / start with Windows: {e}");
                }
            }

            // `--tray` inicia só na bandeja (usado ao iniciar com o Windows).
            // `--tray` starts in the tray only (used when starting with Windows).
            if !std::env::args().any(|a| a == "--tray") {
                window::open(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_state,
            commands::password_strength,
            commands::create_vault,
            commands::unlock,
            commands::recover,
            commands::lock,
            commands::list_items,
            commands::add_item,
            commands::update_item,
            commands::delete_item,
            commands::reveal_password,
            commands::reveal_notes,
            commands::copy_field,
            commands::copy_text,
            commands::generate_password,
            commands::change_master_password,
            commands::regenerate_codes,
            commands::save_settings,
            transfer::export_vault,
            transfer::export_csv,
            transfer::import_file,
            transfer::delete_imported_file,
            transfer::merge_vault,
            transfer::import_vault_file,
            transfer::list_backups,
            transfer::restore_backup,
            bridge::commands::pairing_pending,
            bridge::commands::cancel_pairing,
            bridge::commands::list_pairings,
            bridge::commands::remove_pairing,
            bridge::commands::revoke_site,
            bridge::commands::fill_log,
            commands::reveal_history,
            desktop::quick_context,
            desktop::quick_search,
            desktop::quick_type,
            desktop::quick_save_program,
            desktop::quick_close,
            desktop::hotkeys_in_use,
        ])
        .build(tauri::generate_context!())
        .expect("falha ao iniciar o SigilVault / failed to start SigilVault");

    app.run(|_app, event| {
        // Fechar a última janela não encerra o app: ele continua na bandeja.
        // Closing the last window does not quit: the app stays in the tray.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
