//! Comandos chamados pela interface. Senhas só saem do Rust quando o usuário pede para ver ou copiar.
//! Commands called by the UI. Passwords leave Rust only when the user asks to see or copy them.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sigilvault_core::{
    Error as CoreError, GeneratorOptions, Item, ItemId, ItemKind, ItemUpdate, KdfParams, NewItem, RecoveryCode, Vault,
};
use tauri::{AppHandle, Manager, State};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::rollback::Check;
use crate::settings::{GeneratorPrefs, Settings};
use crate::state::{AppState, Inner, Profile, lock_app};
use crate::{tray, win};

// Lista de comandos (mesma do `build.rs`). / Command list (same as `build.rs`).
include!("command_list.rs");

// ── Erros / Errors ────────────────────────────────────────────────

/// Erro enviado à interface: um código que ela traduz. Nunca contém segredos.
/// Error sent to the UI: a code it translates. Never contains secrets.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdError {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_secs: Option<u64>,
}

impl CmdError {
    fn new(code: &'static str) -> Self {
        Self { code, retry_after_secs: None }
    }

    fn locked_out(wait: Duration) -> Self {
        Self { code: "locked_out", retry_after_secs: Some(wait.as_secs().max(1)) }
    }
}

impl From<CoreError> for CmdError {
    fn from(e: CoreError) -> Self {
        Self::new(match e {
            CoreError::InvalidCredentials => "invalid_credentials",
            CoreError::MalformedRecoveryCode => "malformed_code",
            CoreError::Tampered => "tampered",
            CoreError::InvalidFormat(_) | CoreError::UnsupportedVersion(_) | CoreError::WeakKdfParams => "invalid_file",
            CoreError::WeakMasterPassword => "weak_password",
            CoreError::InvalidGeneratorOptions(_) => "invalid_generator_options",
            CoreError::ItemNotFound => "not_found",
            CoreError::InvalidInput(_) => "invalid_input",
            CoreError::NoRecoveryCodesLeft => "no_codes_left",
            CoreError::Io(_) => "io",
            _ => "internal",
        })
    }
}

impl From<std::io::Error> for CmdError {
    fn from(_: std::io::Error) -> Self {
        Self::new("io")
    }
}

type CmdResult<T> = Result<T, CmdError>;

fn vault_mut(inner: &mut Inner) -> CmdResult<&mut Vault> {
    inner.vault.as_mut().ok_or(CmdError::new("locked"))
}

fn vault_ref(inner: &Inner) -> CmdResult<&Vault> {
    inner.vault.as_ref().ok_or(CmdError::new("locked"))
}

/// Salva o cofre e registra a revisão no guardião. / Saves the vault and records the revision.
fn persist(inner: &mut Inner, state: &AppState) -> CmdResult<()> {
    let Inner { vault, guard, .. } = inner;
    let vault = vault.as_mut().ok_or(CmdError::new("locked"))?;
    vault.save(&state.paths.vault)?;
    guard.record(&vault.vault_id(), vault.revision());
    Ok(())
}

fn codes_to_strings(codes: &[RecoveryCode]) -> Vec<String> {
    codes.iter().map(|c| c.to_display_string().to_string()).collect()
}

// ── Estado / State ────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateDto {
    has_vault: bool,
    unlocked: bool,
    username: Option<String>,
    remaining_codes: Option<usize>,
    retry_after_secs: u64,
    settings: Settings,
}

#[tauri::command(async)]
pub fn app_state(state: State<'_, AppState>) -> AppStateDto {
    let inner = state.lock();
    AppStateDto {
        has_vault: state.paths.vault.exists(),
        unlocked: inner.vault.is_some(),
        username: inner
            .vault
            .as_ref()
            .map(|v| v.username().to_owned())
            .or_else(|| Profile::load(&state.paths).map(|p| p.username)),
        remaining_codes: inner.vault.as_ref().map(Vault::remaining_recovery_codes),
        retry_after_secs: inner.attempts.remaining(Instant::now()).as_secs(),
        settings: inner.settings.clone(),
    }
}

/// Nota de 0 a 4 para o medidor de força. / 0–4 score for the strength meter.
#[tauri::command(async)]
pub fn password_strength(password: String, username: String) -> u8 {
    let password = Zeroizing::new(password);
    sigilvault_core::strength::score(&password, &username)
}

// ── Criar, desbloquear, recuperar, bloquear / Create, unlock, recover, lock ──

#[tauri::command(async)]
pub fn create_vault(app: AppHandle, state: State<'_, AppState>, username: String, password: String) -> CmdResult<Vec<String>> {
    let password = Zeroizing::new(password);
    // Nunca sobrescreve um cofre existente. / Never overwrites an existing vault.
    if state.paths.vault.exists() {
        return Err(CmdError::new("vault_exists"));
    }
    let (vault, codes) = Vault::create(&username, &password, KdfParams::DEFAULT)?;
    let mut inner = state.lock();
    Profile::save(&state.paths, vault.username());
    inner.vault = Some(vault);
    persist(&mut inner, &state)?;
    drop(inner);
    tray::set_locked(&app, false);
    Ok(codes_to_strings(&codes))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockDto {
    /// O arquivo é mais antigo do que o último visto neste PC. / The file is older than the last seen on this PC.
    rollback_warning: bool,
    remaining_codes: usize,
}

#[tauri::command(async)]
pub fn unlock(app: AppHandle, state: State<'_, AppState>, password: String) -> CmdResult<UnlockDto> {
    let password = Zeroizing::new(password);
    let mut inner = state.lock();
    let wait = inner.attempts.remaining(Instant::now());
    if !wait.is_zero() {
        return Err(CmdError::locked_out(wait));
    }
    let vault = match Vault::open_file(&state.paths.vault, &password) {
        Ok(v) => v,
        Err(CoreError::InvalidCredentials) => {
            let wait = inner.attempts.record_failure(Instant::now());
            return Err(if wait.is_zero() { CmdError::new("invalid_credentials") } else { CmdError::locked_out(wait) });
        }
        Err(e) => return Err(e.into()),
    };
    inner.attempts.record_success();
    let rollback_warning = matches!(inner.guard.check(&vault.vault_id(), vault.revision()), Check::Older { .. });
    inner.guard.record(&vault.vault_id(), vault.revision());
    let dto = UnlockDto { rollback_warning, remaining_codes: vault.remaining_recovery_codes() };
    inner.vault = Some(vault);
    drop(inner);
    tray::set_locked(&app, false);
    Ok(dto)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverDto {
    username: String,
    remaining_codes: usize,
}

#[tauri::command(async)]
pub fn recover(app: AppHandle, state: State<'_, AppState>, code: String, new_password: String) -> CmdResult<RecoverDto> {
    let code = Zeroizing::new(code);
    let new_password = Zeroizing::new(new_password);
    let mut inner = state.lock();
    let wait = inner.attempts.remaining(Instant::now());
    if !wait.is_zero() {
        return Err(CmdError::locked_out(wait));
    }
    let bytes = std::fs::read(&state.paths.vault)?;
    let (vault, outcome) = match Vault::recover(&bytes, &code, &new_password) {
        Ok(r) => r,
        Err(CoreError::InvalidCredentials) => {
            let wait = inner.attempts.record_failure(Instant::now());
            return Err(if wait.is_zero() { CmdError::new("invalid_credentials") } else { CmdError::locked_out(wait) });
        }
        Err(e) => return Err(e.into()),
    };
    inner.attempts.record_success();
    inner.vault = Some(vault);
    // Salva já: é isso que destrói o código usado. / Save now: this is what destroys the used code.
    persist(&mut inner, &state)?;
    Profile::save(&state.paths, &outcome.username);
    drop(inner);
    tray::set_locked(&app, false);
    Ok(RecoverDto { username: outcome.username, remaining_codes: outcome.remaining_codes })
}

#[tauri::command(async)]
pub fn lock(app: AppHandle) {
    lock_app(&app, true);
}

// ── Itens / Items ─────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    id: String,
    kind: ItemKind,
    title: String,
    username: String,
    url: String,
    tags: Vec<String>,
    has_password: bool,
    has_notes: bool,
    created_at: u64,
    updated_at: u64,
}

impl From<&Item> for ItemDto {
    fn from(i: &Item) -> Self {
        Self {
            id: i.id().to_string(),
            kind: i.kind(),
            title: i.title().to_owned(),
            username: i.username().to_owned(),
            url: i.url().to_owned(),
            tags: i.tags().to_vec(),
            has_password: i.has_password(),
            has_notes: i.has_notes(),
            created_at: i.created_at(),
            updated_at: i.updated_at(),
        }
    }
}

/// Dados de um item vindos da interface. Em edição: `password`/`notes` ausentes = manter, "" = remover.
/// Item data from the UI. On edit: missing `password`/`notes` = keep, "" = remove.
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    #[zeroize(skip)]
    kind: ItemKind,
    title: String,
    username: String,
    url: String,
    tags: Vec<String>,
    password: Option<String>,
    notes: Option<String>,
}

fn parse_id(id: &str) -> CmdResult<ItemId> {
    id.parse().map_err(|_| CmdError::new("not_found"))
}

#[tauri::command(async)]
pub fn list_items(state: State<'_, AppState>, query: String) -> CmdResult<Vec<ItemDto>> {
    let inner = state.lock();
    let mut items: Vec<ItemDto> = vault_ref(&inner)?.search(&query).into_iter().map(ItemDto::from).collect();
    items.sort_by_key(|i| i.title.to_lowercase());
    Ok(items)
}

#[tauri::command(async)]
pub fn add_item(state: State<'_, AppState>, item: ItemInput) -> CmdResult<String> {
    let mut inner = state.lock();
    let mut new = NewItem::default();
    new.kind = item.kind;
    new.title.clone_from(&item.title);
    new.username.clone_from(&item.username);
    new.url.clone_from(&item.url);
    new.tags.clone_from(&item.tags);
    new.password = item.password.clone().filter(|p| !p.is_empty());
    new.notes = item.notes.clone().filter(|n| !n.is_empty());
    let id = vault_mut(&mut inner)?.add_item(new)?;
    persist(&mut inner, &state)?;
    Ok(id.to_string())
}

#[tauri::command(async)]
pub fn update_item(state: State<'_, AppState>, id: String, item: ItemInput) -> CmdResult<()> {
    let id = parse_id(&id)?;
    let mut inner = state.lock();
    let mut up = ItemUpdate::default();
    up.title.clone_from(&item.title);
    up.username.clone_from(&item.username);
    up.url.clone_from(&item.url);
    up.tags.clone_from(&item.tags);
    up.password.clone_from(&item.password);
    up.notes.clone_from(&item.notes);
    vault_mut(&mut inner)?.update_item(id, up)?;
    persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn delete_item(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    let mut inner = state.lock();
    vault_mut(&mut inner)?.remove_item(id)?;
    persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn reveal_password(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    let id = parse_id(&id)?;
    let inner = state.lock();
    Ok(vault_ref(&inner)?.reveal_password(id)?.map(|p| p.to_string()))
}

#[tauri::command(async)]
pub fn reveal_notes(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    let id = parse_id(&id)?;
    let inner = state.lock();
    Ok(vault_ref(&inner)?.reveal_notes(id)?.map(|n| n.to_string()))
}

// ── Área de transferência / Clipboard ─────────────────────────────

/// Copia com proteção e agenda a limpeza. Devolve os segundos até limpar.
/// Copies with protection and schedules clearing. Returns seconds until clearing.
fn copy_protected(app: &AppHandle, inner: &mut Inner, text: &str) -> CmdResult<u32> {
    let seq = win::clipboard::set_secret_text(win::watcher::hwnd(), text).map_err(|_| CmdError::new("clipboard"))?;
    inner.clipboard_seq = Some(seq);
    let secs = inner.settings.clipboard_clear_secs;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(secs)));
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if inner.clipboard_seq == Some(seq) {
            inner.clipboard_seq = None;
        }
        win::clipboard::clear_if_unchanged(win::watcher::hwnd(), seq);
    });
    Ok(secs)
}

#[tauri::command(async)]
pub fn copy_field(app: AppHandle, state: State<'_, AppState>, id: String, field: String) -> CmdResult<u32> {
    let id = parse_id(&id)?;
    let mut inner = state.lock();
    let vault = vault_ref(&inner)?;
    let text: Zeroizing<String> = match field.as_str() {
        "password" => vault.reveal_password(id)?.ok_or(CmdError::new("not_found"))?,
        "username" => Zeroizing::new(vault.item(id).ok_or(CmdError::new("not_found"))?.username().to_owned()),
        _ => return Err(CmdError::new("invalid_input")),
    };
    copy_protected(&app, &mut inner, &text)
}

#[tauri::command(async)]
pub fn copy_text(app: AppHandle, state: State<'_, AppState>, text: String) -> CmdResult<u32> {
    let text = Zeroizing::new(text);
    let mut inner = state.lock();
    vault_ref(&inner)?;
    copy_protected(&app, &mut inner, &text)
}

// ── Gerador / Generator ───────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedDto {
    password: String,
    entropy_bits: f64,
}

#[tauri::command(async)]
pub fn generate_password(options: GeneratorPrefs) -> CmdResult<GeneratedDto> {
    let opts: GeneratorOptions = options.into();
    let password = sigilvault_core::generate(&opts)?;
    Ok(GeneratedDto { password: password.to_string(), entropy_bits: opts.entropy_bits()? })
}

// ── Senha mestra, códigos e preferências / Master password, codes and settings ──

#[tauri::command(async)]
pub fn change_master_password(state: State<'_, AppState>, current: String, new: String) -> CmdResult<()> {
    let current = Zeroizing::new(current);
    let new = Zeroizing::new(new);
    let mut inner = state.lock();
    vault_mut(&mut inner)?.change_master_password(&current, &new)?;
    persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn regenerate_codes(state: State<'_, AppState>, password: String) -> CmdResult<Vec<String>> {
    let password = Zeroizing::new(password);
    let mut inner = state.lock();
    let codes = vault_mut(&mut inner)?.regenerate_recovery_codes(&password)?;
    persist(&mut inner, &state)?;
    Ok(codes_to_strings(&codes))
}

#[tauri::command(async)]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let settings = settings.sanitized();
    settings.save(&state.paths.settings)?;
    let language_changed = {
        let mut inner = state.lock();
        let changed = inner.settings.language != settings.language;
        inner.settings = settings.clone();
        changed
    };
    if language_changed {
        tray::refresh_menu(&app);
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::COMMANDS;

    /// A janela só pode ter permissão para os nossos comandos, todos eles e nada mais.
    /// The window may only be allowed our commands, all of them and nothing else.
    #[test]
    fn capability_allows_exactly_our_commands() {
        let cap: serde_json::Value = serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let mut granted: Vec<String> =
            cap["permissions"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_owned()).collect();
        let mut expected: Vec<String> = COMMANDS.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
        granted.sort();
        expected.sort();
        assert_eq!(granted, expected);
        assert_eq!(cap["windows"], serde_json::json!(["main-*"]));
    }
}
