//! Estado do app e a rotina central de bloqueio.
//! App state and the central lock routine.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sigilvault_core::Vault;
use tauri::{AppHandle, Manager};

use crate::attempts::Attempts;
use crate::rollback::{Codec, RevisionGuard};
use crate::settings::Settings;
use crate::storage::{Paths, atomic_write};
use crate::{tray, win, window};

/// Índice local do perfil: só o nome, para a tela de login. Nunca vai para exportações.
/// Local profile index: only the name, for the login screen. Never goes into exports.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    pub username: String,
}

impl Profile {
    pub fn load(paths: &Paths) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(&paths.profile).ok()?).ok()
    }

    /// Esquece o perfil (ao importar um cofre de outro PC). / Forgets the profile (when importing a vault from another PC).
    pub fn remove(paths: &Paths) {
        let _ = std::fs::remove_file(&paths.profile);
    }

    pub fn save(paths: &Paths, username: &str) {
        if let Ok(bytes) = serde_json::to_vec(&Self { username: username.to_owned() }) {
            let _ = atomic_write(&paths.profile, &bytes);
        }
    }
}

/// Parte mutável do estado. / Mutable part of the state.
pub struct Inner {
    /// Cofre desbloqueado (None = bloqueado). / Unlocked vault (None = locked).
    pub vault: Option<Vault>,
    pub settings: Settings,
    pub attempts: Attempts,
    pub guard: RevisionGuard,
    /// Marca da última senha copiada, para limpar depois. / Mark of the last copied password, to clear later.
    pub clipboard: Option<win::clipboard::Copied>,
    /// Impressão (SHA-256) do arquivo do cofre como lido/gravado, para detectar conflitos.
    /// Fingerprint (SHA-256) of the vault file as read/written, to detect conflicts.
    pub disk_hash: Option<[u8; 32]>,
    /// Já houve backup nesta sessão? / Was there a backup this session?
    pub backed_up_this_session: bool,
    /// Último arquivo importado (escolhido no diálogo nativo), para oferecer apagá-lo.
    /// Last imported file (picked in the native dialog), to offer deleting it.
    pub last_import: Option<std::path::PathBuf>,
}

/// Estado compartilhado pelos comandos. / State shared by commands.
pub struct AppState {
    pub paths: Paths,
    inner: Mutex<Inner>,
}

impl AppState {
    pub fn new(paths: Paths) -> Self {
        let inner = Inner {
            vault: None,
            settings: Settings::load(&paths.settings),
            attempts: Attempts::load(&paths.attempts, Instant::now()),
            guard: RevisionGuard::load(&paths.revision_guard, Codec::DPAPI),
            clipboard: None,
            disk_hash: None,
            backed_up_this_session: false,
            last_import: None,
        };
        Self { paths, inner: Mutex::new(inner) }
    }

    /// Trava o estado. Se uma thread entrou em pânico, seguimos com o estado (não há invariantes quebrados
    /// que deixem segredos expostos).
    /// Locks the state. If a thread panicked we carry on (no broken invariant exposes secrets).
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Bloqueia o cofre: apaga as chaves da memória, limpa a área de transferência (se ainda for a
/// nossa senha), destrói a janela (WebView2) e troca o ícone da bandeja.
/// Com `reopen`, abre uma janela nova já na tela de desbloqueio.
/// Locks the vault: wipes keys from memory, clears the clipboard (if it still holds our password),
/// destroys the window (WebView2) and switches the tray icon.
/// With `reopen`, opens a fresh window on the unlock screen.
pub fn lock_app(app: &AppHandle, reopen: bool) {
    let state = app.state::<AppState>();
    {
        let mut inner = state.lock();
        // Soltar o Vault dispara o zeroize das chaves e dos dados.
        // Dropping the Vault triggers zeroize of keys and data.
        drop(inner.vault.take());
        inner.disk_hash = None;
        inner.last_import = None;
        if let Some(copied) = inner.clipboard.take() {
            win::clipboard::clear_if_ours(win::watcher::hwnd(), &copied);
        }
    }
    tray::set_locked(app, true);
    window::destroy(app);
    if reopen {
        window::create(app);
    }
}
