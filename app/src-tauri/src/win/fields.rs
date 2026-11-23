//! Campos de senha de outros programas (Fase 7): detecta, mostra o selo do SigilVault ao lado e
//! percebe quando o login aconteceu (para oferecer "Guardar este login?").
//! Password fields of other programs (Phase 7): detects them, shows the SigilVault seal next to
//! them and notices when the login happened (to offer "Keep this login?").
//!
//! Como / How:
//! - O Windows avisa cada mudança de foco (SetWinEventHook, sem injetar nada nos outros programas).
//!   Windows reports each focus change (SetWinEventHook, nothing is injected into other programs).
//! - A UI Automation (a API dos leitores de tela) diz se o campo focado é de senha (`IsPassword`).
//!   UI Automation (the screen-reader API) tells whether the focused field is a password one.
//! - NUNCA lemos o que é digitado nem o conteúdo de campos de senha (o Windows nem deixa). Do campo
//!   de usuário ao lado, só o texto, para sugerir o nome da conta.
//!   We NEVER read what is typed nor the content of password fields (Windows does not even allow
//!   it). From the username field next to it, only its text, to suggest the account name.
//! - Tudo roda numa thread própria: objetos COM, gancho de eventos e a janela do selo.
//!   Everything runs on its own thread: COM objects, the event hook and the seal window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use std::time::Duration;

use sigilvault_ipc::process;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCondition, IUIAutomationElement, IUIAutomationValuePattern,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
};
use windows::core::Interface;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZESTART,
    GA_ROOT, GetAncestor, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, HWND_TOPMOST,
    IDC_HAND, IsWindow, IsWindowVisible, LoadCursorW, MA_NOACTIVATE, MF_STRING, MSG, PostThreadMessageW, RegisterClassW,
    SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_RBUTTONUP, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::target::Target;
use super::wide;

/// O que fazer num programa. / What to do in a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Policy {
    /// Mostrar o selo. / Show the seal.
    pub seal: bool,
    /// Oferecer salvar depois do login. / Offer to save after the login.
    pub offer: bool,
}

/// Campo de senha encontrado (sem a senha, claro). / Password field found (without the password, of course).
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// Identifica este campo nas chamadas seguintes. / Identifies this field in later calls.
    pub token: u64,
    /// Janela principal do programa (raiz). / Program's main (root) window.
    pub target: Target,
    /// Texto do campo de usuário ao lado, se houver. / Text of the nearby username field, if any.
    pub username: String,
    /// Achamos um campo de usuário? / Did we find a username field?
    pub has_username_field: bool,
}

/// Avisos para o app. / Notices to the app.
pub enum Event {
    /// Clicou no selo. / Clicked the seal.
    SealClicked(FieldInfo),
    /// Pediu "não mostrar neste programa". / Asked "do not show in this program".
    Ignore(PathBuf),
    /// O campo de senha sumiu depois de usado: provavelmente entrou. / The password field went away after use: probably signed in.
    LoginDone(FieldInfo),
}

/// Qual campo focar antes de digitar. / Which field to focus before typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Username,
    Password,
}

enum Cmd {
    Focus { token: u64, slot: Slot, reply: SyncSender<bool> },
    MarkFilled(u64),
    HideSeal,
    MenuLabels(String, String),
}

const WM_FOCUS_EVENT: u32 = WM_APP + 1;
const WM_COMMAND_READY: u32 = WM_APP + 2;
const WM_HIDE_EVENT: u32 = WM_APP + 3;
/// Intervalo do relógio do selo/vigia. / Seal/watch timer interval.
const TICK_MS: u32 = 400;
/// Quanto tempo vigiar o campo depois de sair dele (em ticks). / How long to watch the field after leaving it (in ticks).
const WATCH_TICKS: u32 = 50;
/// Maior texto aceito do campo de usuário. / Longest username text accepted.
const MAX_USERNAME: usize = 300;

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static COMMANDS: OnceLock<std::sync::Mutex<Sender<Cmd>>> = OnceLock::new();

type PolicyFn = Box<dyn Fn(&Path) -> Policy + Send>;
type EventFn = Box<dyn Fn(Event) + Send>;

/// Ícone do selo em RGBA (de cima para baixo). / Seal icon in RGBA (top to bottom).
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

struct Current {
    info: FieldInfo,
    password: IUIAutomationElement,
    username: Option<IUIAutomationElement>,
    policy: Policy,
    /// O SigilVault preencheu: não oferecer salvar. / SigilVault filled it: do not offer to save.
    filled: bool,
}

struct Watch {
    field: Current,
    ticks: u32,
}

struct Seal {
    hwnd: HWND,
    icon: Icon,
    visible: bool,
    menu: (String, String),
}

struct Thread {
    uia: IUIAutomation,
    edit: IUIAutomationCondition,
    current: Option<Current>,
    watch: Option<Watch>,
    seal: Seal,
    next_token: u64,
    exe_cache: HashMap<u32, Option<PathBuf>>,
    policy: PolicyFn,
    on_event: EventFn,
    commands: Receiver<Cmd>,
}

thread_local! {
    static STATE: RefCell<Option<Thread>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Thread) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

// ── API pública / Public API ──────────────────────────────────────

/// Inicia o detector numa thread própria. / Starts the detector on its own thread.
pub fn start(icon: Icon, policy: impl Fn(&Path) -> Policy + Send + 'static, on_event: impl Fn(Event) + Send + 'static) {
    let (tx, rx) = channel();
    if COMMANDS.set(std::sync::Mutex::new(tx)).is_err() {
        return;
    }
    let policy: PolicyFn = Box::new(policy);
    let on_event: EventFn = Box::new(on_event);
    let _ = std::thread::Builder::new().name("sigilvault-fields".into()).spawn(move || run(icon, policy, on_event, rx));
}

fn send(cmd: Cmd) -> bool {
    let Some(tx) = COMMANDS.get() else { return false };
    if tx.lock().map(|t| t.send(cmd).is_err()).unwrap_or(true) {
        return false;
    }
    let tid = THREAD_ID.load(Ordering::Acquire);
    // SAFETY: só acorda a thread do detector. / Only wakes the detector thread.
    tid != 0 && unsafe { PostThreadMessageW(tid, WM_COMMAND_READY, 0, 0) } != 0
}

/// Põe o foco no campo (usuário ou senha) do token. Bloqueia até 1,5 s. Não chamar da thread do detector.
/// Focuses the token's field (username or password). Blocks up to 1.5 s. Do not call from the detector thread.
pub fn focus(token: u64, slot: Slot) -> bool {
    let (reply, rx) = sync_channel(1);
    send(Cmd::Focus { token, slot, reply }) && rx.recv_timeout(Duration::from_millis(1500)).unwrap_or(false)
}

/// O SigilVault vai preencher este campo: não oferecer salvar depois. / SigilVault will fill this field: do not offer to save afterwards.
pub fn mark_filled(token: u64) {
    send(Cmd::MarkFilled(token));
}

/// Esconde o selo agora. / Hides the seal now.
pub fn hide_seal() {
    send(Cmd::HideSeal);
}

/// Textos do menu do selo (no idioma do app). / Seal menu texts (in the app language).
pub fn set_menu_labels(open: &str, ignore: &str) {
    send(Cmd::MenuLabels(open.to_owned(), ignore.to_owned()));
}

// ── Thread do detector / Detector thread ──────────────────────────

unsafe extern "system" fn on_win_event(_: HWINEVENTHOOK, event: u32, hwnd: HWND, _id_object: i32, _id_child: i32, _thread: u32, _time: u32) {
    // Só agenda o trabalho: nada de UI Automation dentro do gancho. / Only schedules the work: no UI Automation inside the hook.
    let msg = if event == EVENT_OBJECT_FOCUS || event == EVENT_SYSTEM_FOREGROUND { WM_FOCUS_EVENT } else { WM_HIDE_EVENT };
    // SAFETY: mensagem para a própria thread. / Message to our own thread.
    unsafe { PostThreadMessageW(GetCurrentThreadId(), msg, hwnd as usize, 0) };
}

/// Diagnóstico só em depuração (arquivo em %TEMP%; nunca com conteúdo de campos).
/// Debug-only diagnostics (file in %TEMP%; never with field contents).
pub fn trace(msg: &str) {
    if cfg!(debug_assertions) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("sigilvault-fields.log")) {
            let _ = writeln!(f, "{msg}");
        }
    }
}

fn run(icon: Icon, policy: PolicyFn, on_event: EventFn, commands: Receiver<Cmd>) {
    trace("start");
    // SAFETY: inicialização COM desta thread (MTA); objetos COM só usados nela.
    // SAFETY: COM initialization of this thread (MTA); COM objects only used on it.
    let uia: IUIAutomation = unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
            trace(&format!("CoInitializeEx: {e:?}"));
            return;
        }
        match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(u) => u,
            Err(e) => {
                trace(&format!("CUIAutomation: {e:?}"));
                return;
            }
        }
    };
    // Programa travado não pode travar o SigilVault: tempos curtos. / A hung program must not hang SigilVault: short timeouts.
    if let Ok(u2) = uia.cast::<IUIAutomation2>() {
        // SAFETY: chamadas simples de configuração. / Simple configuration calls.
        unsafe {
            let _ = u2.SetConnectionTimeout(1000);
            let _ = u2.SetTransactionTimeout(1000);
        }
    }
    // SAFETY: cria a condição "tipo = caixa de texto". / Creates the "type = edit box" condition.
    let edit = match unsafe { uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0)) } {
        Ok(c) => c,
        Err(e) => {
            trace(&format!("condição / condition: {e:?}"));
            return;
        }
    };
    let Some(seal_hwnd) = create_seal_window() else {
        trace("janela do selo / seal window");
        return;
    };
    trace("pronto / ready");

    STATE.with(|s| {
        *s.borrow_mut() = Some(Thread {
            uia,
            edit,
            current: None,
            watch: None,
            seal: Seal { hwnd: seal_hwnd, icon, visible: false, menu: ("SigilVault".into(), "—".into()) },
            next_token: 1,
            exe_cache: HashMap::new(),
            policy,
            on_event,
            commands,
        })
    });

    // SAFETY: gancho fora de processo (nada é injetado nos outros programas); eventos chegam a esta
    // thread pela fila de mensagens. / Out-of-context hook (nothing is injected into other
    // programs); events reach this thread through the message queue.
    unsafe {
        THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, std::ptr::null_mut(), Some(on_win_event), 0, 0, flags);
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, std::ptr::null_mut(), Some(on_win_event), 0, 0, flags);
        SetWinEventHook(EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZESTART, std::ptr::null_mut(), Some(on_win_event), 0, 0, flags);
        SetWinEventHook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZESTART, std::ptr::null_mut(), Some(on_win_event), 0, 0, flags);
        SetTimer(seal_hwnd, 1, TICK_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            match msg.message {
                WM_FOCUS_EVENT => {
                    // Junta rajadas de eventos: só o último foco importa. / Coalesces bursts: only the last focus matters.
                    with(|t| t.on_focus());
                }
                WM_HIDE_EVENT => {
                    with(|t| t.hide_seal());
                }
                WM_COMMAND_READY => {
                    with(|t| t.drain_commands());
                }
                _ => {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

impl Thread {
    fn exe_of(&mut self, pid: u32) -> Option<PathBuf> {
        if self.exe_cache.len() > 256 {
            self.exe_cache.clear();
        }
        self.exe_cache.entry(pid).or_insert_with(|| process::image_path(pid).ok()).clone()
    }

    fn emit(&self, e: Event) {
        (self.on_event)(e);
    }

    fn same(&self, a: &IUIAutomationElement, b: &IUIAutomationElement) -> bool {
        // SAFETY: comparação de dois elementos válidos. / Comparison of two valid elements.
        unsafe { self.uia.CompareElements(a, b) }.map(|r| r.as_bool()).unwrap_or(false)
    }

    /// O foco mudou em algum programa. / Focus changed in some program.
    fn on_focus(&mut self) {
        // SAFETY: consultas à UI Automation (com tempo limite) e a janelas. / UI Automation (with timeout) and window queries.
        let focused = unsafe { self.uia.GetFocusedElement() };
        if let Err(e) = &focused {
            trace(&format!("foco sem elemento / focus without element: {e:?}"));
        }
        let Some(el) = focused.ok() else { return self.left_field() };
        let is_password = unsafe { el.CurrentIsPassword() }.map(|b| b.as_bool()).unwrap_or(false)
            && unsafe { el.CurrentIsEnabled() }.map(|b| b.as_bool()).unwrap_or(false);
        if !is_password {
            return self.left_field();
        }
        let pid = unsafe { el.CurrentProcessId() }.unwrap_or(0) as u32;
        if pid == 0 || pid == std::process::id() {
            return self.left_field();
        }

        // Mesmo campo de antes (voltou a ele): só reposiciona. / Same field as before (came back): just repositions.
        if let Some(cur) = &self.current
            && self.same(&cur.password, &el)
        {
            return self.place_seal();
        }
        if let Some(w) = self.watch.take() {
            if self.same(&w.field.password, &el) {
                self.current = Some(w.field);
                return self.place_seal();
            }
            self.watch = Some(w);
        }

        // O programa é o dono da JANELA em primeiro plano (o foco está sempre nela), não o processo
        // que desenha o campo: em apps com WebView2 o campo vem do msedgewebview2.exe, comum a todos.
        // The program is the owner of the foreground WINDOW (focus is always in it), not the process
        // drawing the field: in WebView2 apps the field comes from msedgewebview2.exe, shared by all.
        let root = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(root, &mut owner) };
        // Janela do próprio SigilVault (ex.: senha mestra): nunca. / SigilVault's own window (e.g. master password): never.
        if owner == 0 || owner == std::process::id() {
            return self.left_field();
        }
        let Some(exe) = self.exe_of(owner) else { return self.left_field() };
        let title = window_title(root);
        let target = Target { hwnd: root as usize, pid: owner, exe: exe.clone(), title };
        if target.is_browser() {
            return self.left_field();
        }
        let policy = (self.policy)(&exe);
        trace(&format!("campo de senha / password field: exe={} seal={} offer={}", target.exe_name(), policy.seal, policy.offer));
        if !policy.seal && !policy.offer {
            return self.left_field();
        }
        // Campo novo. / New field.
        self.hide_seal();
        let username = self.find_username(&el);
        let text = username.as_ref().map(value_of).unwrap_or_default();
        let token = self.next_token;
        self.next_token += 1;
        self.current = Some(Current {
            info: FieldInfo { token, target, username: text, has_username_field: username.is_some() },
            password: el,
            username,
            policy,
            filled: false,
        });
        self.place_seal();
    }

    /// Saiu do campo de senha: começa a vigiar se o login aconteceu. / Left the password field: starts watching whether the login happened.
    fn left_field(&mut self) {
        self.hide_seal();
        if let Some(mut cur) = self.current.take() {
            if let Some(u) = &cur.username {
                let v = value_of(u);
                if !v.is_empty() {
                    cur.info.username = v;
                }
            }
            if cur.policy.offer && !cur.filled {
                self.watch = Some(Watch { field: cur, ticks: 0 });
            }
        }
    }

    /// Campo de usuário: a caixa de texto (não de senha) mais próxima ANTES do campo de senha.
    /// Username field: the nearest text box (not a password one) BEFORE the password field.
    fn find_username(&self, password: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        // SAFETY: navegação na árvore de acessibilidade, com tempo limite. / Accessibility tree navigation, with timeout.
        unsafe {
            let walker = self.uia.ControlViewWalker().ok()?;
            let mut ancestor = walker.GetParentElement(password).ok()?;
            for _ in 0..4 {
                if let Ok(all) = ancestor.FindAll(TreeScope_Descendants, &self.edit) {
                    let n = all.Length().unwrap_or(0).min(64);
                    let mut best: Option<IUIAutomationElement> = None;
                    for i in 0..n {
                        let Ok(e) = all.GetElement(i) else { continue };
                        if self.same(&e, password) {
                            if best.is_some() {
                                return best;
                            }
                            break;
                        }
                        let pw = e.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true);
                        let on = e.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
                        if !pw && on {
                            best = Some(e);
                        }
                    }
                }
                ancestor = walker.GetParentElement(&ancestor).ok()?;
            }
            None
        }
    }

    /// Relógio: acompanha o campo (selo) e o login (vigia). / Timer: follows the field (seal) and the login (watch).
    fn tick(&mut self) {
        // Relê o usuário enquanto o campo de senha está em foco: se a janela de login fechar de
        // repente, o último valor já está guardado. / Re-reads the username while the password
        // field has focus: if the login window closes suddenly, the last value is already kept.
        if let Some(cur) = &mut self.current
            && let Some(u) = &cur.username
        {
            let v = value_of(u);
            if !v.is_empty() {
                cur.info.username = v;
            }
        }
        if self.seal.visible {
            let still = self.current.as_ref().is_some_and(|c| {
                // SAFETY: consultas simples. / Simple queries.
                unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) as usize == c.info.target.hwnd }
            });
            if !still {
                trace("selo: outra janela em primeiro plano / seal: another foreground window");
            }
            if still { self.place_seal() } else { self.hide_seal() }
        }
        let Some(w) = &mut self.watch else { return };
        w.ticks += 1;
        // Confere a cada ~1,2 s. / Checks every ~1.2 s.
        if w.ticks % 3 != 0 {
            return;
        }
        let hwnd = w.field.info.target.hwnd as HWND;
        // SAFETY: consultas sobre uma janela e um elemento que podem não existir mais.
        // SAFETY: queries about a window and an element that may no longer exist.
        let gone = unsafe {
            IsWindow(hwnd) == 0
                || IsWindowVisible(hwnd) == 0
                || w.field.password.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true)
        };
        if gone {
            let w = self.watch.take().expect("vigia / watch");
            // Havia campo de usuário mas ficou vazio: provavelmente só fechou a janela, sem entrar.
            // There was a username field but it stayed empty: probably just closed the window, without signing in.
            trace(&format!("campo sumiu / field gone: usuário/username={} campo/field={}", !w.field.info.username.is_empty(), w.field.info.has_username_field));
            if !(w.field.info.has_username_field && w.field.info.username.is_empty()) {
                self.emit(Event::LoginDone(w.field.info));
            }
        } else if w.ticks >= WATCH_TICKS {
            self.watch = None;
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(cmd) = self.commands.try_recv() {
            match cmd {
                Cmd::Focus { token, slot, reply } => {
                    let field = self.current.as_ref().filter(|c| c.info.token == token).or_else(|| self.watch.as_ref().map(|w| &w.field).filter(|f| f.info.token == token));
                    let el = field.and_then(|f| match slot {
                        Slot::Password => Some(f.password.clone()),
                        Slot::Username => f.username.clone(),
                    });
                    // SAFETY: pede ao programa que foque o próprio campo. / Asks the program to focus its own field.
                    let ok = el.is_some_and(|e| unsafe { e.SetFocus() }.is_ok());
                    let _ = reply.send(ok);
                }
                Cmd::MarkFilled(token) => {
                    if let Some(c) = self.current.as_mut().filter(|c| c.info.token == token) {
                        c.filled = true;
                    }
                    if self.watch.as_ref().is_some_and(|w| w.field.info.token == token) {
                        self.watch = None;
                    }
                }
                Cmd::HideSeal => self.hide_seal(),
                Cmd::MenuLabels(open, ignore) => self.seal.menu = (open, ignore),
            }
        }
    }

    // ── Selo / Seal ──

    fn place_seal(&mut self) {
        let Some(cur) = &self.current else { return self.hide_seal() };
        if !cur.policy.seal {
            return self.hide_seal();
        }
        // SAFETY: retângulo do campo em pixels físicos (o app é ciente de DPI). / Field rectangle in physical pixels (the app is DPI aware).
        let Ok(r) = (unsafe { cur.password.CurrentBoundingRectangle() }) else { return self.hide_seal() };
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w < 40 || h < 12 {
            return self.hide_seal();
        }
        // SAFETY: DPI da janela do programa. / DPI of the program's window.
        let dpi = unsafe { GetDpiForWindow(cur.info.target.hwnd as HWND) }.max(96) as i32;
        let size = (22 * dpi / 96).min(h - 2).max(14);
        let x = r.right - size - 5 * dpi / 96;
        let y = r.top + (h - size) / 2;
        if !self.seal.visible {
            trace(&format!("mostra o selo / shows the seal at {x},{y} size {size}"));
        }
        draw_seal(&self.seal, x, y, size);
        self.seal.visible = true;
    }

    fn hide_seal(&mut self) {
        if self.seal.visible {
            trace("esconde o selo / hides the seal");
            // SAFETY: esconde a nossa janela. / Hides our window.
            unsafe { ShowWindow(self.seal.hwnd, SW_HIDE) };
            self.seal.visible = false;
        }
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: buffer do tamanho informado. / Buffer of the given size.
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..len])
}

/// Texto de uma caixa de texto (nunca de senha). / Text of a text box (never a password one).
fn value_of(el: &IUIAutomationElement) -> String {
    // SAFETY: lê o valor pelo padrão de valor da UI Automation, com tempo limite. / Reads the value through UI Automation's value pattern, with timeout.
    unsafe {
        if el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
            return String::new();
        }
        let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) else { return String::new() };
        let v = p.CurrentValue().map(|b| b.to_string()).unwrap_or_default();
        v.trim().chars().take(MAX_USERNAME).collect()
    }
}

// ── Janela do selo / Seal window ──────────────────────────────────

unsafe extern "system" fn seal_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // Clicar no selo não tira o foco do campo do programa. / Clicking the seal does not take focus from the program's field.
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_SETCURSOR => {
            // SAFETY: cursor de mão do sistema. / System hand cursor.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND)) };
            return 1;
        }
        WM_TIMER => {
            with(|t| t.tick());
            return 0;
        }
        WM_LBUTTONUP => {
            let info = with(|t| {
                t.hide_seal();
                t.current.as_ref().map(|c| c.info.clone())
            })
            .flatten();
            if let Some(info) = info {
                with(|t| t.emit(Event::SealClicked(info)));
            }
            return 0;
        }
        WM_RBUTTONUP => {
            let data = with(|t| (t.current.as_ref().map(|c| c.info.clone()), t.seal.menu.clone())).and_then(|(i, m)| i.map(|i| (i, m)));
            if let Some((info, (open, ignore))) = data {
                // O menu roda o próprio laço de mensagens: nada emprestado durante ele.
                // The menu runs its own message loop: nothing borrowed during it.
                match show_menu(hwnd, &open, &ignore) {
                    1 => {
                        with(|t| {
                            t.hide_seal();
                            t.emit(Event::SealClicked(info));
                        });
                    }
                    2 => {
                        with(|t| {
                            t.hide_seal();
                            t.current = None;
                            t.emit(Event::Ignore(info.target.exe));
                        });
                    }
                    _ => {}
                }
            }
            return 0;
        }
        _ => {}
    }
    // SAFETY: tratamento padrão. / Default handling.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn show_menu(hwnd: HWND, open: &str, ignore: &str) -> i32 {
    let (open, ignore) = (wide(open), wide(ignore));
    // SAFETY: menu criado, mostrado e destruído aqui. / Menu created, shown and destroyed here.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        AppendMenuW(menu, MF_STRING, 1, open.as_ptr());
        AppendMenuW(menu, MF_STRING, 2, ignore.as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
        DestroyMenu(menu);
        cmd
    }
}

fn create_seal_window() -> Option<HWND> {
    let class = wide("SigilVaultSeal");
    // SAFETY: classe e janela criadas nesta thread, que bombeia mensagens. / Class and window created on this thread, which pumps messages.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(seal_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        (!hwnd.is_null()).then_some(hwnd)
    }
}

/// Reduz a imagem por média de área e converte para BGRA pré-multiplicado (formato do Windows).
/// Downscales the image by area averaging and converts to premultiplied BGRA (Windows' format).
fn scaled_bgra(icon: &Icon, size: u32) -> Vec<u8> {
    let (sw, sh) = (icon.width as f32, icon.height as f32);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (x0, x1) = (x as f32 * sw / size as f32, (x + 1) as f32 * sw / size as f32);
            let (y0, y1) = (y as f32 * sh / size as f32, (y + 1) as f32 * sh / size as f32);
            let mut acc = [0f32; 4];
            let mut n = 0f32;
            for sy in y0 as u32..(y1.ceil() as u32).min(icon.height) {
                for sx in x0 as u32..(x1.ceil() as u32).min(icon.width) {
                    let i = ((sy * icon.width + sx) * 4) as usize;
                    let a = icon.rgba[i + 3] as f32 / 255.0;
                    // Média em cor pré-multiplicada (evita halo escuro). / Average in premultiplied color (avoids a dark halo).
                    acc[0] += icon.rgba[i] as f32 * a;
                    acc[1] += icon.rgba[i + 1] as f32 * a;
                    acc[2] += icon.rgba[i + 2] as f32 * a;
                    acc[3] += a;
                    n += 1.0;
                }
            }
            let o = ((y * size + x) * 4) as usize;
            if n > 0.0 {
                out[o] = (acc[2] / n).round() as u8;
                out[o + 1] = (acc[1] / n).round() as u8;
                out[o + 2] = (acc[0] / n).round() as u8;
                out[o + 3] = (acc[3] / n * 255.0).round() as u8;
            }
        }
    }
    out
}

fn draw_seal(seal: &Seal, x: i32, y: i32, size: i32) {
    let pixels = scaled_bgra(&seal.icon, size as u32);
    // SAFETY: bitmap de memória do tamanho exato, liberado no fim; janela em camadas nossa.
    // SAFETY: memory bitmap of the exact size, freed at the end; our own layered window.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if !bmp.is_null() && !bits.is_null() {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            let old = SelectObject(mem, bmp);
            let pos = POINT { x, y };
            let sz = SIZE { cx: size, cy: size };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            SetWindowPos(seal.hwnd, HWND_TOPMOST, x, y, size, size, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            UpdateLayeredWindow(seal.hwnd, screen, &pos, &sz, mem, &src, 0, &blend, ULW_ALPHA);
            SelectObject(mem, old);
            DeleteObject(bmp);
        }
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_keeps_alpha_and_premultiplies() {
        // 2×2 → 1×1: dois pixels vermelhos opacos e dois transparentes. / Two opaque red and two transparent pixels.
        let icon = Icon { rgba: vec![255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0], width: 2, height: 2 };
        let out = scaled_bgra(&icon, 1);
        assert_eq!(out[3], 128); // metade opaco / half opaque
        assert_eq!(out[2], 128); // vermelho pré-multiplicado / premultiplied red
        assert_eq!((out[0], out[1]), (0, 0));
    }
}
