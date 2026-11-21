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
use crate::state::{AppState, Inner, Profile, announce_lock_state, lock_app};
use crate::storage::{atomic_write, sha256};
use crate::{backups, tray, win};

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
    pub(crate) fn new(code: &'static str) -> Self {
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
            CoreError::UnsupportedImport(_) => "unsupported_import",
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

pub(crate) type CmdResult<T> = Result<T, CmdError>;

pub(crate) fn vault_mut(inner: &mut Inner) -> CmdResult<&mut Vault> {
    inner.vault.as_mut().ok_or(CmdError::new("locked"))
}

pub(crate) fn vault_ref(inner: &Inner) -> CmdResult<&Vault> {
    inner.vault.as_ref().ok_or(CmdError::new("locked"))
}

/// Salva o cofre e registra a revisão no guardião. Antes, confere se o arquivo no disco ainda é
/// o que lemos: se outro programa (ou outro PC, via sincronização) mudou o cofre, recusa sobrescrever.
/// Saves the vault and records the revision. First checks the file on disk is still what we read:
/// if another program (or another PC, via sync) changed the vault, it refuses to overwrite.
pub(crate) fn persist(inner: &mut Inner, state: &AppState) -> CmdResult<()> {
    if let Some(expected) = inner.disk_hash
        && std::fs::read(&state.paths.vault).ok().map(|b| sha256(&b)) != Some(expected)
    {
        return Err(CmdError::new("conflict"));
    }
    let Inner { vault, guard, disk_hash, .. } = inner;
    let vault = vault.as_mut().ok_or(CmdError::new("locked"))?;
    let bytes = vault.to_bytes()?;
    atomic_write(&state.paths.vault, &bytes)?;
    *disk_hash = Some(sha256(&bytes));
    guard.record(&vault.vault_id(), vault.revision());
    Ok(())
}

/// Faz um backup antes de mudar o cofre: sempre, se a mudança for importante; senão, só na
/// primeira mudança da sessão. Melhor esforço: falhar aqui não impede a edição.
/// Makes a backup before changing the vault: always for important changes; otherwise only on the
/// session's first change. Best effort: failing here does not block editing.
pub(crate) fn backup_before(inner: &mut Inner, state: &AppState, important: bool) {
    if !important && inner.backed_up_this_session {
        return;
    }
    if let Some(vault) = inner.vault.as_ref()
        && let Ok(bytes) = vault.create_backup()
        && backups::write(&state.paths.backups, &bytes).is_ok()
    {
        inner.backed_up_this_session = true;
    }
}

/// Depois de trocar a chave do cofre: troca a chave dos backups, salva o cofre e regrava os
/// backups (os ilegíveis são apagados).
/// After rotating the vault key: rotates the backup key, saves the vault and rewrites the
/// backups (unreadable ones are deleted).
pub(crate) fn rekey_backups_and_persist(inner: &mut Inner, state: &AppState) -> CmdResult<()> {
    let files = backups::list(&state.paths.backups);
    let contents: Vec<Vec<u8>> = files.iter().map(|p| std::fs::read(p).unwrap_or_default()).collect();
    let rekeyed = vault_mut(inner)?.rekey_backups(&contents)?;
    persist(inner, state)?;
    for (path, new) in files.iter().zip(rekeyed) {
        match new {
            Some(bytes) => {
                let _ = atomic_write(path, &bytes);
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
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
    announce_lock_state(&app, false);
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
    let bytes = std::fs::read(&state.paths.vault)?;
    let vault = match Vault::open(&bytes, &password) {
        Ok(v) => v,
        Err(CoreError::InvalidCredentials) => {
            let wait = inner.attempts.record_failure(Instant::now());
            return Err(if wait.is_zero() { CmdError::new("invalid_credentials") } else { CmdError::locked_out(wait) });
        }
        Err(e) => return Err(e.into()),
    };
    inner.attempts.record_success();
    inner.disk_hash = Some(sha256(&bytes));
    inner.backed_up_this_session = false;
    Profile::save(&state.paths, vault.username());
    let rollback_warning = matches!(inner.guard.check(&vault.vault_id(), vault.revision()), Check::Older { .. });
    inner.guard.record(&vault.vault_id(), vault.revision());
    let dto = UnlockDto { rollback_warning, remaining_codes: vault.remaining_recovery_codes() };
    inner.vault = Some(vault);
    drop(inner);
    announce_lock_state(&app, false);
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
    inner.disk_hash = Some(sha256(&bytes));
    inner.backed_up_this_session = false;
    // Salva já: é isso que destrói o código usado. A chave dos backups também é trocada.
    // Save now: this is what destroys the used code. The backup key is rotated too.
    rekey_backups_and_persist(&mut inner, &state)?;
    Profile::save(&state.paths, &outcome.username);
    drop(inner);
    announce_lock_state(&app, false);
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
    match_subdomains: bool,
    history_len: usize,
    /// Programa: caminho do executável. / Program: executable path.
    app_path: String,
    /// Programa: a assinatura foi registrada? / Program: was the signature recorded?
    app_signed: bool,
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
            match_subdomains: i.match_subdomains(),
            history_len: i.history_len(),
            app_path: i.app_path().to_owned(),
            app_signed: i.app_signer().is_some(),
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
    #[zeroize(skip)]
    #[serde(default)]
    match_subdomains: bool,
    #[serde(default)]
    app_path: String,
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
    new.match_subdomains = item.match_subdomains;
    new.app_path.clone_from(&item.app_path);
    vault_ref(&inner)?;
    backup_before(&mut inner, &state, false);
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
    up.match_subdomains = item.match_subdomains;
    up.app_path.clone_from(&item.app_path);
    // A assinatura guardada só vale para o mesmo executável; trocou o caminho, ela some.
    // The stored signature only applies to the same executable; if the path changed, it is dropped.
    {
        let current = vault_ref(&inner)?.item(id).ok_or(CmdError::new("not_found"))?;
        if sigilvault_core::normalize_path(current.app_path()) == sigilvault_core::normalize_path(&item.app_path) {
            up.app_signer = current.app_signer().map(str::to_owned);
        }
    }
    vault_ref(&inner)?;
    backup_before(&mut inner, &state, false);
    vault_mut(&mut inner)?.update_item(id, up)?;
    persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn delete_item(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    let mut inner = state.lock();
    vault_ref(&inner)?;
    backup_before(&mut inner, &state, true);
    vault_mut(&mut inner)?.remove_item(id)?;
    persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn reveal_password(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    let id = parse_id(&id)?;
    let inner = state.lock();
    Ok(vault_ref(&inner)?.reveal_password(id)?.map(|p| p.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDto {
    changed_at: u64,
    password: String,
}

/// Senhas anteriores de um item (só quando o usuário pede). / An item's previous passwords (only when the user asks).
#[tauri::command(async)]
pub fn reveal_history(state: State<'_, AppState>, id: String) -> CmdResult<Vec<HistoryDto>> {
    let id = parse_id(&id)?;
    let inner = state.lock();
    Ok(vault_ref(&inner)?
        .reveal_history(id)?
        .into_iter()
        .map(|(changed_at, p)| HistoryDto { changed_at, password: p.to_string() })
        .collect())
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
    let copied = win::clipboard::set_secret_text(win::watcher::hwnd(), text).map_err(|_| CmdError::new("clipboard"))?;
    inner.clipboard = Some(copied.clone());
    let secs = inner.settings.clipboard_clear_secs;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(secs)));
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        // Só limpa se esta ainda for a cópia mais recente. / Only clears if this is still the latest copy.
        if inner.clipboard.as_ref() == Some(&copied) {
            inner.clipboard = None;
            win::clipboard::clear_if_ours(win::watcher::hwnd(), &copied);
        }
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
    vault_mut(&mut inner)?.verify_master_password(&current)?;
    backup_before(&mut inner, &state, true);
    vault_mut(&mut inner)?.change_master_password(&current, &new)?;
    rekey_backups_and_persist(&mut inner, &state)
}

#[tauri::command(async)]
pub fn regenerate_codes(state: State<'_, AppState>, password: String) -> CmdResult<Vec<String>> {
    let password = Zeroizing::new(password);
    let mut inner = state.lock();
    vault_mut(&mut inner)?.verify_master_password(&password)?;
    backup_before(&mut inner, &state, true);
    let codes = vault_mut(&mut inner)?.regenerate_recovery_codes(&password)?;
    rekey_backups_and_persist(&mut inner, &state)?;
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
    crate::desktop::apply_hotkeys(&app, &settings);
    crate::apply_startup(settings.start_with_windows)?;
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
        // Única permissão além dos comandos: ouvir eventos (aviso de pareamento).
        // Only permission besides the commands: listening to events (pairing notice).
        expected.extend(["core:event:allow-listen".to_owned(), "core:event:allow-unlisten".to_owned()]);
        granted.sort();
        expected.sort();
        assert_eq!(granted, expected);
        assert_eq!(cap["windows"], serde_json::json!(["main-*"]));
    }

    /// O pop-up rápido só pode o mínimo: buscar, desbloquear, gerar, copiar e digitar.
    /// The quick popup may only do the minimum: search, unlock, generate, copy and type.
    #[test]
    fn quick_popup_capability_is_minimal() {
        let cap: serde_json::Value = serde_json::from_str(include_str!("../capabilities/quick.json")).unwrap();
        let mut granted: Vec<&str> = cap["permissions"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
        granted.sort();
        let mut expected = vec![
            "allow-unlock", "allow-generate-password", "allow-copy-field", "allow-quick-context", "allow-quick-search",
            "allow-quick-type", "allow-quick-save-program", "allow-quick-close",
        ];
        expected.sort();
        assert_eq!(granted, expected);
        assert_eq!(cap["windows"], serde_json::json!(["quick-*"]));
    }

    /// Toda permissão da pasta `capabilities` precisa estar ativada no `tauri.conf.json`
    /// (senão a janela fica sem poder chamar nada).
    /// Every capability in the `capabilities` folder must be enabled in `tauri.conf.json`
    /// (otherwise the window cannot call anything).
    #[test]
    fn all_capabilities_are_enabled() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let enabled = &conf["app"]["security"]["capabilities"];
        for id in ["main", "quick"] {
            assert!(enabled.as_array().unwrap().iter().any(|c| c == id), "{id}");
        }
    }
}
