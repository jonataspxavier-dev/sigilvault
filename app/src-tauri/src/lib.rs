//! App desktop do SigilVault: serviço da bandeja + janela Tauri.
//! SigilVault desktop app: tray service + Tauri window.

mod attempts;
mod backups;
mod bridge;
mod commands;
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

            // `--tray` inicia só na bandeja (para iniciar com o Windows, no futuro).
            // `--tray` starts in the tray only (for starting with Windows, later).
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
