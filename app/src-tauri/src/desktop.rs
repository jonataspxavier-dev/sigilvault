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

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use sigilvault_core::{ItemId, ItemKind, NewItem};
use sigilvault_ipc::signature::{self, Signer};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use zeroize::Zeroizing;

use crate::commands::{CmdError, CmdResult, backup_before, persist, vault_ref};
use crate::settings::Settings;
use crate::state::AppState;
use crate::platform::fields::{self, Slot};
use crate::platform::hotkeys::{self, Action};
use crate::platform::target::Target;
use crate::platform::typer::{self, Part, TypeError};
use crate::window;

/// Etiqueta das entradas criadas pelo atalho de gerar, até o usuário confirmar.
/// Tag of the entries created by the generate hotkey, until the user confirms.
pub const PENDING_TAG: &str = "pendente";

/// Ícone do selo dos campos de senha (dragão com contorno dourado). / Password field seal icon (gold-outlined dragon).
const SEAL_ICON: &[u8] = include_bytes!("../icons/seal-64.png");
/// No máximo um "Guardar este login?" por programa neste intervalo. / At most one "Keep this login?" per program in this interval.
const OFFER_EVERY: Duration = Duration::from_secs(10 * 60);

/// Como o pop-up foi aberto. / How the popup was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Search,
    Generate,
    /// Aviso "Guardar este login?". / "Keep this login?" notice.
    Offer,
}

/// Campo de senha de onde veio o pedido (selo ou aviso). / Password field the request came from (seal or notice).
#[derive(Debug, Clone)]
struct FieldRef {
    token: u64,
    has_username: bool,
}

/// Sessão do pop-up aberto. / Session of the open popup.
struct Session {
    mode: Mode,
    target: Option<Target>,
    field: Option<FieldRef>,
    /// Usuário sugerido (do campo ao lado). / Suggested username (from the nearby field).
    prefill: String,
}

/// Estado do desktop (gerenciado pelo Tauri). / Desktop state (managed by Tauri).
#[derive(Default)]
pub struct Desktop {
    session: Mutex<Option<Session>>,
    /// Atalhos que o Windows recusou (em uso por outro programa). / Hotkeys Windows refused.
    in_use: Mutex<Vec<&'static str>>,
    /// Último aviso por programa. / Last notice per program.
    offered: Mutex<HashMap<String, Instant>>,
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
    start_fields(app, &settings);
}

/// Liga o detector de campos de senha dos programas. / Starts the programs' password field detector.
fn start_fields(app: &AppHandle, settings: &Settings) {
    let img = match tauri::image::Image::from_bytes(SEAL_ICON) {
        Ok(i) => i,
        Err(e) => {
            fields::trace(&format!("ícone do selo / seal icon: {e}"));
            return;
        }
    };
    let icon = fields::Icon { rgba: img.rgba().to_vec(), width: img.width(), height: img.height() };
    let (for_policy, for_events) = (app.clone(), app.clone());
    fields::start(
        icon,
        move |exe| {
            let state = for_policy.state::<AppState>();
            let inner = state.lock();
            let s = &inner.settings;
            let ignored = s.desktop_ignored.contains(&sigilvault_core::normalize_path(&exe.to_string_lossy()));
            fields::Policy { seal: s.desktop_seal && !ignored, offer: s.desktop_offer && !ignored }
        },
        move |event| {
            let app = for_events.clone();
            // Fora da thread do detector (ela não pode esperar). / Off the detector thread (it must not wait).
            std::thread::spawn(move || on_field_event(&app, event));
        },
    );
    apply_language(&settings.language);
}

/// Textos do menu do selo no idioma escolhido. / Seal menu texts in the chosen language.
pub fn apply_language(language: &str) {
    if language == "en" {
        fields::set_menu_labels("Open SigilVault here", "Do not show in this program");
    } else {
        fields::set_menu_labels("Abrir o SigilVault aqui", "Não mostrar neste programa");
    }
}

fn on_field_event(app: &AppHandle, event: fields::Event) {
    match event {
        fields::Event::SealClicked(info) => {
            *app.state::<Desktop>().session() = Some(Session {
                mode: Mode::Search,
                field: Some(FieldRef { token: info.token, has_username: info.has_username_field }),
                prefill: info.username,
                target: Some(info.target),
            });
            window::open_quick(app);
        }
        fields::Event::Ignore(exe) => ignore_program(app, &exe.to_string_lossy()),
        fields::Event::LoginDone(info) => offer_save(app, info),
    }
}

/// Inclui o programa na lista de ignorados (sem selo e sem aviso). / Adds the program to the ignored list (no seal, no notice).
fn ignore_program(app: &AppHandle, exe: &str) {
    let state = app.state::<AppState>();
    let mut settings = state.lock().settings.clone();
    settings.desktop_ignored.push(exe.to_owned());
    let settings = settings.sanitized();
    if settings.save(&state.paths.settings).is_ok() {
        state.lock().settings = settings;
    }
}

/// Mostra "Guardar este login?" se fizer sentido. / Shows "Keep this login?" if it makes sense.
fn offer_save(app: &AppHandle, info: fields::FieldInfo) {
    let exe = info.target.exe.to_string_lossy().into_owned();
    let key = sigilvault_core::normalize_path(&exe);
    {
        let state = app.state::<AppState>();
        let inner = state.lock();
        if !inner.settings.desktop_offer || inner.settings.desktop_ignored.contains(&key) {
            return;
        }
        // Já guardado para este programa (mesmo usuário, ou qualquer um se não soubermos o usuário).
        // Already saved for this program (same username, or any if we do not know the username).
        if let Some(v) = inner.vault.as_ref() {
            let saved = v.programs_for(&exe);
            if saved.iter().any(|i| info.username.is_empty() || i.username().eq_ignore_ascii_case(&info.username)) {
                return;
            }
        }
    }
    let desktop = app.state::<Desktop>();
    {
        let mut offered = desktop.offered.lock().unwrap_or_else(|e| e.into_inner());
        if offered.get(&key).is_some_and(|t| t.elapsed() < OFFER_EVERY) {
            return;
        }
        offered.insert(key, Instant::now());
    }
    // Não atropela um pop-up que a pessoa está usando. / Does not run over a popup the person is using.
    if app.webview_windows().keys().any(|l| l.starts_with("quick-")) {
        return;
    }
    fields::trace("mostra o aviso / shows the notice");
    *desktop.session() = Some(Session { mode: Mode::Offer, target: Some(info.target), field: None, prefill: info.username });
    window::open_offer(app);
}

/// Lê `--quick search` ou `--quick generate` da linha de comando. É o atalho que funciona em
/// qualquer ambiente: no Linux (Wayland, principalmente), a pessoa liga uma tecla do sistema a
/// esse comando. / Reads `--quick search` or `--quick generate` from the command line. It is the
/// hotkey that works everywhere: on Linux (Wayland mostly), the person binds a system key to it.
pub fn quick_from_args(args: &[String]) -> Option<Action> {
    let pos = args.iter().position(|a| a == "--quick")?;
    match args.get(pos + 1).map(String::as_str) {
        Some("search") => Some(Action::Search),
        Some("generate") => Some(Action::Generate),
        _ => None,
    }
}

/// Dispara o pop-up como se o atalho tivesse sido apertado agora (pela linha de comando).
/// Triggers the popup as if the hotkey had just been pressed (from the command line).
pub fn trigger(app: &AppHandle, action: Action) {
    let app = app.clone();
    // O alvo é lido antes de o pop-up existir, e fora da thread que chamou. / The target is read before the popup exists, off the calling thread.
    std::thread::spawn(move || {
        let target = crate::platform::target::foreground();
        on_hotkey(&app, action, target);
    });
}

fn on_hotkey(app: &AppHandle, action: Action, target: Option<Target>) {
    #[cfg(debug_assertions)]
    eprintln!("[sigilvault] atalho {action:?}, alvo: {:?}", target.as_ref().map(|t| t.exe_name()));
    // Atalho apertado dentro do próprio SigilVault: sem alvo (só copiar). / Hotkey inside SigilVault itself: no target (copy only).
    let target = target.filter(|t| !t.is_ours());
    let mode = if action == Action::Generate { Mode::Generate } else { Mode::Search };
    *app.state::<Desktop>().session() = Some(Session { mode, target, field: None, prefill: String::new() });
    fields::hide_seal();
    window::open_quick(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_the_quick_command_line() {
        assert_eq!(quick_from_args(&args(&["app", "--quick", "search"])), Some(Action::Search));
        assert_eq!(quick_from_args(&args(&["app", "--tray", "--quick", "generate"])), Some(Action::Generate));
        assert_eq!(quick_from_args(&args(&["app", "--quick"])), None);
        assert_eq!(quick_from_args(&args(&["app", "--quick", "outra"])), None);
        assert_eq!(quick_from_args(&args(&["app", "--tray"])), None);
    }
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
    /// Usuário sugerido (aviso ou selo). / Suggested username (notice or seal).
    prefill_username: String,
    /// Aberto pelo selo: preenche campo a campo. / Opened by the seal: fills field by field.
    from_field: bool,
}

/// Para onde o pop-up vai digitar e em que modo. / Where the popup will type and in which mode.
#[tauri::command]
pub fn quick_context(state: State<'_, AppState>, desktop: State<'_, Desktop>) -> QuickContext {
    let session = desktop.session();
    let (mode, target) = match session.as_ref() {
        Some(s) => (
            match s.mode {
                Mode::Search => "search",
                Mode::Generate => "generate",
                Mode::Offer => "offer",
            },
            s.target.as_ref(),
        ),
        None => ("search", None),
    };
    let prefill_username = session.as_ref().map(|s| s.prefill.clone()).unwrap_or_default();
    let from_field = session.as_ref().is_some_and(|s| s.field.is_some());
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
        prefill_username,
        from_field,
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
    let (target, field) = {
        let session = desktop.session();
        (typable(session.as_ref().and_then(|s| s.target.as_ref()))?, session.as_ref().and_then(|s| s.field.clone()))
    };
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
    type_and_close(&window, &target, &fill_parts(field.as_ref(), username, password))
}

/// Blocos para preencher. Pelo selo: foca cada campo pela UI Automation e substitui o conteúdo
/// (não depende de Tab). Pelo atalho: usuário + Tab + senha, como antes.
/// Blocks to fill. From the seal: focuses each field through UI Automation and replaces its
/// content (no Tab). From the hotkey: username + Tab + password, as before.
fn fill_parts(field: Option<&FieldRef>, username: Zeroizing<String>, password: Zeroizing<String>) -> Vec<Part> {
    let mut parts = Vec::new();
    match field {
        Some(f) => {
            // Preenchido pelo SigilVault: não perguntar "guardar?" depois. / Filled by SigilVault: do not ask "keep?" afterwards.
            fields::mark_filled(f.token);
            let token = f.token;
            if f.has_username && !username.is_empty() {
                parts.push(Part::Focus(Box::new(move || fields::focus(token, Slot::Username))));
                parts.push(Part::SelectAll);
                parts.push(Part::Text(username));
            }
            parts.push(Part::Focus(Box::new(move || fields::focus(token, Slot::Password))));
            parts.push(Part::SelectAll);
            parts.push(Part::Text(password));
        }
        None => {
            if !username.is_empty() {
                parts.push(Part::Text(username));
                parts.push(Part::Tab);
            }
            parts.push(Part::Text(password));
        }
    }
    parts
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
    let (target_ref, field) = {
        let session = desktop.session();
        (session.as_ref().and_then(|s| s.target.clone()), session.as_ref().and_then(|s| s.field.clone()))
    };
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
        // Pelo selo, preenche também o usuário digitado no pop-up. / From the seal, also fills the username typed in the popup.
        let username = Zeroizing::new(if field.is_some() { username.trim().to_owned() } else { String::new() });
        type_and_close(&window, &target, &fill_parts(field.as_ref(), username, password))?;
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

/// Aviso aceito: vira o pop-up normal, já na aba "Só salvar". / Notice accepted: becomes the regular popup, on the "Just save" tab.
#[tauri::command]
pub fn quick_offer_accept(window: WebviewWindow, desktop: State<'_, Desktop>) {
    if let Some(s) = desktop.session().as_mut() {
        s.mode = Mode::Search;
    }
    window::grow_offer(&window);
}

/// "Nunca neste programa": ignora o programa do aviso e fecha. / "Never in this program": ignores the notice's program and closes.
#[tauri::command]
pub fn quick_ignore_program(app: AppHandle, window: WebviewWindow, desktop: State<'_, Desktop>) {
    let exe = desktop.session().take().and_then(|s| s.target).map(|t| t.exe.to_string_lossy().into_owned());
    if let Some(exe) = exe {
        ignore_program(&app, &exe);
    }
    let _ = window.destroy();
}

/// Atalhos recusados pelo Windows (para a tela de Configurações). / Hotkeys refused by Windows (for Settings).
#[tauri::command]
pub fn hotkeys_in_use(desktop: State<'_, Desktop>) -> Vec<&'static str> {
    desktop.in_use.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
