//! Comandos da janela para o pareamento. / Window commands for pairing.

use std::time::Instant;

use serde::Serialize;
use sigilvault_core::hex;
use tauri::{AppHandle, Manager, State};

use super::Bridge;
use crate::commands::{CmdError, CmdResult};
use crate::state::AppState;

fn require_unlocked(state: &AppState) -> CmdResult<()> {
    if state.lock().vault.is_some() { Ok(()) } else { Err(CmdError::new("locked")) }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDto {
    code: String,
    browser: String,
    expires_in_secs: u64,
    attempts_left: u8,
}

/// Pareamento pendente, com o código para mostrar (só com o cofre aberto).
/// Pending pairing, with the code to show (only with the vault unlocked).
#[tauri::command]
pub fn pairing_pending(state: State<'_, AppState>, bridge: State<'_, Bridge>) -> CmdResult<Option<PendingDto>> {
    require_unlocked(&state)?;
    let mut b = bridge.lock();
    b.expire_pending();
    Ok(b.pending.as_ref().map(|p| PendingDto {
        code: p.code.to_display_string(),
        browser: p.browser.clone(),
        expires_in_secs: p.expires.saturating_duration_since(Instant::now()).as_secs(),
        attempts_left: p.attempts_left,
    }))
}

/// Cancela o pareamento pendente. / Cancels the pending pairing.
#[tauri::command]
pub fn cancel_pairing(app: AppHandle) {
    super::cancel_pending(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingDto {
    id: String,
    browser: String,
    created_at: u64,
    last_seen: u64,
    connected: bool,
}

/// Extensões pareadas. / Paired extensions.
#[tauri::command]
pub fn list_pairings(state: State<'_, AppState>, bridge: State<'_, Bridge>) -> CmdResult<Vec<PairingDto>> {
    require_unlocked(&state)?;
    let mut b = bridge.lock();
    let connected: Vec<[u8; 16]> = b.live_conns().iter().filter_map(|c| c.pairing_id()).collect();
    Ok(b.pairings
        .list()
        .iter()
        .map(|p| PairingDto {
            id: hex::encode(&p.id),
            browser: p.browser.clone(),
            created_at: p.created_at,
            last_seen: p.last_seen,
            connected: connected.contains(&p.id),
        })
        .collect())
}

/// Remove um pareamento: a extensão perde o acesso na hora. / Removes a pairing: the extension loses access immediately.
#[tauri::command]
pub fn remove_pairing(app: AppHandle, id: String) -> CmdResult<()> {
    require_unlocked(&app.state::<AppState>())?;
    let id: [u8; 16] = hex::decode(&id).and_then(|v| v.try_into().ok()).ok_or(CmdError::new("invalid_input"))?;
    super::remove_pairing(&app, &id)?;
    Ok(())
}
