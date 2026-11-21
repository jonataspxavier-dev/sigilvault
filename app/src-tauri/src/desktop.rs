//! Desktop (Fase 6): atalhos globais, pop-up rápido e Auto-Type.
//! Desktop (Phase 6): global hotkeys, quick popup and Auto-Type.
//!
//! Fluxo / Flow:
//! 1. Atalho → captura a janela ativa (executável, PID, HWND) → abre o pop-up.
//!    Hotkey → captures the active window (executable, PID, HWND) → opens the popup.
//! 2. Busca: primeiro as entradas daquele EXECUTÁVEL (nunca pelo título); depois as outras.
//!    Search: first the entries for that EXECUTABLE (never by title); then the others.
//! 3. Digitar: confere assinatura, recusa navegador e janela de administrador, fecha o pop-up e
//!    digita usuário + Tab + senha conferindo a janela antes de cada bloco.
//!    Type: checks the signature, refuses browsers and administrator windows, closes the popup
//!    and types username + Tab + password checking the window before each block.
//! 4. Gerar: salva a entrada como "pendente" ANTES de digitar (se algo falhar, a senha não se perde).
//!    Generate: saves the entry as "pending" BEFORE typing (if something fails, the password is not lost).

use std::sync::Mutex;

use serde::Serialize;
use sigilvault_core::{ItemId, ItemKind, NewItem};
use sigilvault_ipc::signature::{self, Signer};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use zeroize::Zeroizing;

use crate::commands::{CmdError, CmdResult, backup_before, persist, vault_ref};
use crate::settings::Settings;
use crate::state::AppState;
use crate::win::hotkeys::{self, Action};
use crate::win::target::Target;
use crate::win::typer::{self, Part, TypeError};
use crate::window;

/// Etiqueta das entradas criadas pelo atalho de gerar, até o usuário confirmar.
/// Tag of the entries created by the generate hotkey, until the user confirms.
pub const PENDING_TAG: &str = "pendente";

/// Sessão do pop-up aberto. / Session of the open popup.
struct Session {
    action: Action,
    target: Option<Target>,
}

/// Estado do desktop (gerenciado pelo Tauri). / Desktop state (managed by Tauri).
#[derive(Default)]
pub struct Desktop {
    session: Mutex<Option<Session>>,
    /// Atalhos que o Windows recusou (em uso por outro programa). / Hotkeys Windows refused.
    in_use: Mutex<Vec<&'static str>>,
}

impl Desktop {
    fn session(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Liga os atalhos das preferências. Devolve os nomes dos que já estavam em uso.
/// Turns on the hotkeys from the settings. Returns the names of those already in use.
pub fn apply_hotkeys(app: &AppHandle, settings: &Settings) -> Vec<&'static str> {
    let results = hotkeys::apply(vec![
        (Action::Search, hotkeys::parse(&settings.hotkey_search)),
        (Action::Generate, hotkeys::parse(&settings.hotkey_generate)),
    ]);
    let in_use: Vec<&'static str> = results
        .into_iter()
        .filter(|(_, ok)| !ok)
        .map(|(a, _)| if a == Action::Search { "search" } else { "generate" })
        .collect();
    *app.state::<Desktop>().in_use.lock().unwrap_or_else(|e| e.into_inner()) = in_use.clone();
    in_use
}

/// Inicia os atalhos globais. / Starts the global hotkeys.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    hotkeys::start(move |action, target| {
        let app = handle.clone();
        // Não trava a fila de mensagens dos atalhos. / Does not block the hotkey message queue.
        std::thread::spawn(move || on_hotkey(&app, action, target));
    });
    let settings = app.state::<AppState>().lock().settings.clone();
    apply_hotkeys(app, &settings);
}

fn on_hotkey(app: &AppHandle, action: Action, target: Option<Target>) {
    #[cfg(debug_assertions)]
    eprintln!("[sigilvault] atalho {action:?}, alvo: {:?}", target.as_ref().map(|t| t.exe_name()));
    // Atalho apertado dentro do próprio SigilVault: sem alvo (só copiar). / Hotkey inside SigilVault itself: no target (copy only).
    let target = target.filter(|t| !t.is_ours());
    *app.state::<Desktop>().session() = Some(Session { action, target });
    window::open_quick(app);
}

// ── Comandos do pop-up / Popup commands ───────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDto {
    exe_name: String,
    exe_path: String,
    title: String,
    browser: bool,
    elevated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickContext {
    mode: &'static str,
    target: Option<TargetDto>,
    unlocked: bool,
    username: Option<String>,
    language: String,
    generator: crate::settings::GeneratorPrefs,
}

/// Para onde o pop-up vai digitar e em que modo. / Where the popup will type and in which mode.
#[tauri::command]
pub fn quick_context(state: State<'_, AppState>, desktop: State<'_, Desktop>) -> QuickContext {
    let session = desktop.session();
    let (mode, target) = match session.as_ref() {
        Some(s) => (if s.action == Action::Generate { "generate" } else { "search" }, s.target.as_ref()),
        None => ("search", None),
    };
    let inner = state.lock();
    QuickContext {
        mode,
        target: target.map(|t| TargetDto {
            exe_name: t.exe_name(),
            exe_path: t.exe.to_string_lossy().into_owned(),
            title: t.title.chars().take(80).collect(),
            browser: t.is_browser(),
            elevated: t.elevated(),
        }),
        unlocked: inner.vault.is_some(),
        username: inner.vault.as_ref().map(|v| v.username().to_owned()),
        language: inner.settings.language.clone(),
        generator: inner.settings.generator,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickItem {
    id: String,
    title: String,
    username: String,
    kind: ItemKind,
    /// Salvo para o programa ativo. / Saved for the active program.
    matched: bool,
}

/// Busca: primeiro o que foi salvo para o executável ativo; notas nunca aparecem.
/// Search: first what was saved for the active executable; notes never show up.
#[tauri::command]
pub fn quick_search(state: State<'_, AppState>, desktop: State<'_, Desktop>, query: String) -> CmdResult<Vec<QuickItem>> {
    let exe = desktop.session().as_ref().and_then(|s| s.target.as_ref()).map(|t| t.exe.to_string_lossy().into_owned());
    let inner = state.lock();
    let v = vault_ref(&inner)?;
    let matched: Vec<ItemId> = exe.as_deref().map(|e| v.programs_for(e).iter().map(|i| i.id()).collect()).unwrap_or_default();
    let mut list: Vec<QuickItem> = v
        .search(&query)
        .into_iter()
        .filter(|i| i.kind() != ItemKind::SecureNote && i.has_password())
        .map(|i| QuickItem { id: i.id().to_string(), title: i.title().to_owned(), username: i.username().to_owned(), kind: i.kind(), matched: matched.contains(&i.id()) })
        .collect();
    list.sort_by_key(|i| !i.matched);
    list.truncate(30);
    Ok(list)
}

fn type_error(e: TypeError) -> CmdError {
    CmdError::new(match e {
        TypeError::KeysHeld => "keys_held",
        TypeError::WindowChanged => "window_changed",
        TypeError::Blocked => "type_blocked",
    })
}

/// Confere se dá para digitar no alvo. / Checks whether the target can be typed into.
fn typable(target: Option<&Target>) -> CmdResult<Target> {
    let t = target.ok_or(CmdError::new("no_target"))?;
    // Programa fechado: dizer isso (e não "administrador", que é o que a consulta de privilégio
    // concluiria por segurança). / Program closed: say so (not "administrator", which the
    // privilege query would conclude for safety).
    if !t.exists() {
        return Err(CmdError::new("target_gone"));
    }
    if t.is_browser() {
        return Err(CmdError::new("browser_target"));
    }
    if t.elevated() {
        return Err(CmdError::new("elevated_target"));
    }
    Ok(t.clone())
}

/// Esconde o pop-up, digita e fecha. Se falhar, o pop-up volta com o erro.
/// Hides the popup, types and closes. If it fails, the popup comes back with the error.
fn type_and_close(window: &WebviewWindow, target: &Target, parts: &[Part]) -> CmdResult<()> {
    let _ = window.hide();
    match typer::type_into(target, parts) {
        Ok(()) => {
            let _ = window.destroy();
            Ok(())
        }
        Err(e) => {
            let _ = window.show();
            let _ = window.set_focus();
            Err(type_error(e))
        }
    }
}

/// Auto-Type de uma entrada: usuário + Tab + senha (só a senha se não houver usuário).
/// Auto-Type of an entry: username + Tab + password (only the password if there is no username).
#[tauri::command(async)]
pub fn quick_type(window: WebviewWindow, state: State<'_, AppState>, desktop: State<'_, Desktop>, id: String) -> CmdResult<()> {
    let target = typable(desktop.session().as_ref().and_then(|s| s.target.as_ref()))?;
    let id: ItemId = id.parse().map_err(|_| CmdError::new("not_found"))?;
    let (username, password) = {
        let inner = state.lock();
        let v = vault_ref(&inner)?;
        let item = v.item(id).ok_or(CmdError::new("not_found"))?;
        // Programa assinado: a assinatura do executável ativo tem que ser a mesma de quando foi salvo.
        // Signed program: the active executable's signature must match the one saved.
        if item.kind() == ItemKind::Program
            && let Some(expected) = item.app_signer()
            && sigilvault_core::normalize_path(item.app_path()) == sigilvault_core::normalize_path(&target.exe.to_string_lossy())
            && signer_hex(&target) != Some(expected.to_owned())
        {
            return Err(CmdError::new("signature_changed"));
        }
        let password = v.reveal_password(id)?.ok_or(CmdError::new("not_found"))?;
        (Zeroizing::new(item.username().to_owned()), password)
    };
    let mut parts = Vec::new();
    if !username.is_empty() {
        parts.push(Part::Text(username));
        parts.push(Part::Tab);
    }
    parts.push(Part::Text(password));
    type_and_close(&window, &target, &parts)
}

/// SHA-256 (hex) do certificado de quem assinou o executável, se assinado e válido.
/// SHA-256 (hex) of the executable signer's certificate, if signed and valid.
fn signer_hex(target: &Target) -> Option<String> {
    match signature::signer(&target.exe) {
        Signer::Trusted(hash) => Some(sigilvault_core::hex::encode(&hash)),
        _ => None,
    }
}

/// Salva uma entrada de Programa para o alvo; com `type_it`, ela fica "pendente" e a senha é
/// digitada DEPOIS de salva. / Saves a Program entry for the target; with `type_it`, it is marked
/// "pending" and the password is typed AFTER saving.
#[tauri::command(async)]
pub fn quick_save_program(
    window: WebviewWindow,
    state: State<'_, AppState>,
    desktop: State<'_, Desktop>,
    title: String,
    username: String,
    password: String,
    type_it: bool,
) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    if password.is_empty() || password.len() > 1024 || title.len() > 200 || username.len() > 300 {
        return Err(CmdError::new("invalid_input"));
    }
    let target_ref = desktop.session().as_ref().and_then(|s| s.target.clone());
    let target = if type_it { typable(target_ref.as_ref())? } else { target_ref.ok_or(CmdError::new("no_target"))? };
    if target.is_browser() {
        return Err(CmdError::new("browser_target"));
    }
    let mut item = NewItem::default();
    item.kind = ItemKind::Program;
    item.title = if title.trim().is_empty() { target.exe_name() } else { title.trim().to_owned() };
    item.username = username.trim().to_owned();
    item.password = Some(password.to_string());
    item.app_path = target.exe.to_string_lossy().into_owned();
    item.app_signer = signer_hex(&target);
    if type_it {
        item.tags = vec![PENDING_TAG.to_owned()];
    }
    let id = {
        let mut inner = state.lock();
        vault_ref(&inner)?;
        backup_before(&mut inner, &state, false);
        let v = inner.vault.as_mut().ok_or(CmdError::new("locked"))?;
        let id = v.add_item(item)?;
        persist(&mut inner, &state)?;
        id
    };
    if type_it {
        type_and_close(&window, &target, &[Part::Text(password)])?;
    } else {
        let _ = window.destroy();
    }
    Ok(id.to_string())
}

/// Fecha o pop-up. / Closes the popup.
#[tauri::command]
pub fn quick_close(window: WebviewWindow, desktop: State<'_, Desktop>) {
    *desktop.session() = None;
    let _ = window.destroy();
}

/// Atalhos recusados pelo Windows (para a tela de Configurações). / Hotkeys refused by Windows (for Settings).
#[tauri::command]
pub fn hotkeys_in_use(desktop: State<'_, Desktop>) -> Vec<&'static str> {
    desktop.in_use.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
