//! Operações do navegador (Fase 5): consultar logins do site, preencher, salvar e gerar senhas.
//! Browser operations (Phase 5): query the site's logins, fill, save and generate passwords.
//!
//! Regras / Rules:
//! - A origem vem do navegador (a extensão a lê de `sender`, nunca da página) e o app confere de
//!   novo a regra de origem do núcleo antes de entregar qualquer senha.
//!   The origin comes from the browser (the extension reads it from `sender`, never from the page)
//!   and the app re-checks the core origin rule before handing out any password.
//! - Nunca existe "listar todas as senhas": só os logins que servem para aquela origem, e sem senha.
//!   There is never "list all passwords": only the logins that fit that origin, without passwords.
//! - Primeiro preenchimento num site: confirmação no app. "Sempre" fica lembrado por pareamento.
//!   Dentro de iframe de outro site: pergunta toda vez. / First fill on a site: confirmation in
//!   the app. "Always" is remembered per pairing. Inside another site's iframe: asks every time.
//! - Limite de frequência e registro de cada preenchimento. / Rate limit and a log of every fill.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sigilvault_core::{GeneratorOptions, ItemId, ItemUpdate, NewItem, Origin, Vault, strength};
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use super::log::FillEntry;
use super::pairings::now_secs;
use super::session::Conn;
use super::Bridge;
use crate::commands::{backup_before, persist};
use crate::dialogs::{self, FillChoice};
use crate::settings::{GeneratorPrefs, Settings};
use crate::state::AppState;

/// Preenchimentos por minuto, por pareamento. / Fills per minute, per pairing.
const FILLS_PER_MINUTE: usize = 20;
const MAX_TEXT: usize = 1024;

fn err(code: &str) -> Value {
    json!({ "error": code })
}

fn text<'a>(req: &'a Value, key: &str) -> Option<&'a str> {
    req.get(key).and_then(Value::as_str).filter(|s| s.len() <= MAX_TEXT)
}

/// Origem do quadro (frame) e da aba (topo). / Frame origin and tab (top) origin.
fn origins(req: &Value) -> Option<(Origin, Origin)> {
    // Só origens completas vindas do navegador (com protocolo). / Only full origins from the browser (with scheme).
    let parse = |k: &str| text(req, k).filter(|s| s.contains("://")).and_then(Origin::parse);
    Some((parse("origin")?, parse("top")?))
}

fn lang(app: &AppHandle) -> String {
    app.state::<AppState>().lock().settings.language.clone()
}

fn browser_of(app: &AppHandle, conn: &Conn) -> String {
    let Some(id) = conn.pairing_id() else { return String::new() };
    app.state::<Bridge>().lock().pairings.list().iter().find(|p| p.id == id).map(|p| p.browser.clone()).unwrap_or_default()
}

fn login_summary(v: &Vault, origin: &Origin) -> Vec<Value> {
    v.logins_for(origin).iter().map(|i| json!({ "id": i.id().to_string(), "title": i.title(), "username": i.username() })).collect()
}

/// Logins que servem para a página (sem senha). / Logins that fit the page (no password).
pub fn logins(app: &AppHandle, req: &Value) -> Value {
    let Some((origin, top)) = origins(req) else { return err("bad_origin") };
    let state = app.state::<AppState>();
    let inner = state.lock();
    let Some(v) = inner.vault.as_ref() else { return json!({ "locked": true }) };
    json!({
        "locked": false,
        "site": origin.host(),
        "secure": origin.is_secure(),
        "crossOrigin": origin != top,
        "logins": login_summary(v, &origin),
    })
}

/// Entrega usuário e senha de um login, depois das checagens e (se preciso) da confirmação.
/// Hands over a login's username and password, after the checks and (if needed) confirmation.
pub fn fill(app: &AppHandle, conn: &Conn, req: &Value) -> Value {
    let Some(pairing) = conn.pairing_id() else { return err("not_authenticated") };
    if !app.state::<Bridge>().lock().fill_limits.entry(pairing).or_insert_with(FillLimiter::new).allow(FILLS_PER_MINUTE) {
        return err("rate_limited");
    }
    let Some((origin, top)) = origins(req) else { return err("bad_origin") };
    let Some(id) = text(req, "id").and_then(|s| s.parse::<ItemId>().ok()) else { return err("not_found") };
    let site = origin.to_canonical();
    let cross = origin != top;

    // 1. O item existe e serve para esta origem? (Sem abrir a senha ainda.)
    //    Does the item exist and fit this origin? (Without opening the password yet.)
    let (title, username) = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let Some(v) = inner.vault.as_ref() else { return json!({ "locked": true }) };
        let Some(item) = v.item(id) else { return err("not_found") };
        if !item.origin().is_some_and(|o| o.fills(&origin, item.match_subdomains())) {
            return err("not_allowed");
        }
        (item.title().to_owned(), item.username().to_owned())
    };

    // 2. Confirmação no app, se o site ainda não foi liberado (ou se é iframe de outro site).
    //    Confirmation in the app, if the site was not allowed yet (or it is another site's iframe).
    let browser = browser_of(app, conn);
    let allowed = !cross && app.state::<Bridge>().lock().pairings.is_allowed(&pairing, &site);
    if !allowed {
        let lang = lang(app);
        let who = if username.is_empty() { String::new() } else { format!(" ({username})") };
        let mut message = dialogs::tr(
            &lang,
            &format!("O {browser} quer preencher o login \"{title}\"{who} em:\n\n{site}\n\nSó permita se foi você quem clicou no SigilVault agora."),
            &format!("{browser} wants to fill the login \"{title}\"{who} on:\n\n{site}\n\nOnly allow it if you just clicked SigilVault."),
        );
        if cross {
            message.push_str(&dialogs::tr(
                &lang,
                &format!("\n\nAtenção: o formulário está dentro de outro site ({}). Por isso a permissão vale só desta vez.", top.to_canonical()),
                &format!("\n\nWarning: the form is inside another site ({}). So the permission is for this time only.", top.to_canonical()),
            ));
        }
        match dialogs::ask_fill(app, &lang, &message, !cross) {
            FillChoice::Cancel => return err("denied"),
            FillChoice::Once => {}
            FillChoice::Always => {
                let _ = app.state::<Bridge>().lock().pairings.allow_site(&pairing, &site);
            }
        }
    }

    // 3. Só agora abre a senha (o cofre pode ter trancado durante a pergunta).
    //    Only now opens the password (the vault may have locked during the question).
    let password = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let Some(v) = inner.vault.as_ref() else { return json!({ "locked": true }) };
        match v.reveal_password(id) {
            Ok(Some(p)) => p,
            _ => return err("not_found"),
        }
    };
    app.state::<Bridge>().lock().log.push(FillEntry { at: now_secs(), browser, origin: site, title, username: username.clone() });
    json!({ "username": username, "password": password.as_str() })
}

/// Antes de oferecer "Salvar?": o login enviado é novo, mudou ou já está igual?
/// Before offering "Save?": is the submitted login new, changed or already the same?
pub fn check_save(app: &AppHandle, req: &Value) -> Value {
    let Some((origin, _)) = origins(req) else { return err("bad_origin") };
    let (Some(username), Some(password)) = (text(req, "username"), text(req, "password")) else { return err("bad_message") };
    let password = Zeroizing::new(password.to_owned());
    let state = app.state::<AppState>();
    let inner = state.lock();
    let Some(v) = inner.vault.as_ref() else { return json!({ "locked": true }) };
    let wanted = username.trim().to_lowercase();
    let same_user = v.logins_for(&origin).into_iter().find(|i| i.username().trim().to_lowercase() == wanted);
    let Some(item) = same_user else {
        return json!({ "locked": false, "status": "new", "site": origin.host() });
    };
    let same = v.reveal_password(item.id()).ok().flatten().is_some_and(|p| p.as_str() == password.as_str());
    json!({
        "locked": false,
        "status": if same { "same" } else { "changed" },
        "site": origin.host(),
        "item": { "id": item.id().to_string(), "title": item.title(), "username": item.username() },
    })
}

/// Salva (novo) ou troca a senha de um login existente (a antiga vai para o histórico).
/// Só acontece depois de um clique real do usuário no balão da extensão.
/// Saves (new) or changes the password of an existing login (the old one goes to the history).
/// Only happens after a real user click in the extension's balloon.
pub fn save(app: &AppHandle, req: &Value) -> Value {
    let Some((origin, _)) = origins(req) else { return err("bad_origin") };
    let (Some(username), Some(password)) = (text(req, "username"), text(req, "password")) else { return err("bad_message") };
    if password.is_empty() {
        return err("bad_message");
    }
    let password = Zeroizing::new(password.to_owned());
    let title = text(req, "title").map(str::trim).filter(|t| !t.is_empty() && t.len() <= 128).unwrap_or(origin.host()).to_owned();
    let replace = text(req, "replace").and_then(|s| s.parse::<ItemId>().ok());

    let state = app.state::<AppState>();
    let mut inner = state.lock();
    let Some(v) = inner.vault.as_ref() else { return json!({ "locked": true }) };
    let (outcome, id) = match replace {
        Some(id) => {
            let Some(item) = v.item(id) else { return err("not_found") };
            if !item.origin().is_some_and(|o| o.fills(&origin, item.match_subdomains())) {
                return err("not_allowed");
            }
            let mut update = ItemUpdate::default();
            update.title = item.title().to_owned();
            update.url = item.url().to_owned();
            update.tags = item.tags().to_vec();
            update.username = username.to_owned();
            update.password = Some(password.to_string());
            update.match_subdomains = item.match_subdomains();
            update.app_path = item.app_path().to_owned();
            update.app_signer = item.app_signer().map(str::to_owned);
            backup_before(&mut inner, &state, false);
            let Some(v) = inner.vault.as_mut() else { return json!({ "locked": true }) };
            if v.update_item(id, update).is_err() {
                return err("internal");
            }
            ("updated", id)
        }
        None => {
            let new = NewItem::login(&title, username, &origin.to_canonical(), &password);
            backup_before(&mut inner, &state, false);
            let Some(v) = inner.vault.as_mut() else { return json!({ "locked": true }) };
            match v.add_item(new) {
                Ok(id) => ("added", id),
                Err(_) => return err("internal"),
            }
        }
    };
    match persist(&mut inner, &state) {
        Ok(()) => json!({ "saved": true, "outcome": outcome, "id": id.to_string() }),
        Err(_) => err("save_failed"),
    }
}

/// Opções do gerador vindas da extensão, ou as salvas como padrão. / Generator options from the extension, or the saved defaults.
fn generator_prefs(app: &AppHandle, req: &Value) -> GeneratorPrefs {
    req.get("options")
        .and_then(|o| serde_json::from_value::<GeneratorPrefs>(o.clone()).ok())
        .unwrap_or_else(|| app.state::<AppState>().lock().settings.generator)
}

/// Gera uma senha (o mesmo gerador do app). / Generates a password (the app's own generator).
pub fn generate(app: &AppHandle, req: &Value) -> Value {
    let options: GeneratorOptions = generator_prefs(app, req).into();
    match (sigilvault_core::generate(&options), options.entropy_bits()) {
        (Ok(p), Ok(bits)) => json!({ "password": p.as_str(), "entropyBits": bits }),
        _ => err("invalid_generator_options"),
    }
}

/// Opções padrão do gerador (para o balão abrir com elas). / Default generator options (for the balloon to open with).
pub fn get_generator(app: &AppHandle) -> Value {
    json!({ "options": app.state::<AppState>().lock().settings.generator })
}

/// Guarda as opções preferidas como padrão (o planejamento pede). / Saves the preferred options as default (the plan asks for it).
pub fn save_generator(app: &AppHandle, req: &Value) -> Value {
    let Some(prefs) = req.get("options").and_then(|o| serde_json::from_value::<GeneratorPrefs>(o.clone()).ok()) else { return err("bad_message") };
    let state = app.state::<AppState>();
    let settings: Settings = {
        let inner = state.lock();
        Settings { generator: prefs, ..inner.settings.clone() }.sanitized()
    };
    if settings.save(&state.paths.settings).is_err() {
        return err("save_failed");
    }
    let options = settings.generator;
    state.lock().settings = settings;
    json!({ "options": options })
}

/// Força de uma senha digitada pelo usuário ("só salvar"). / Strength of a user-typed password ("just save").
pub fn strength(req: &Value) -> Value {
    let password = Zeroizing::new(text(req, "password").unwrap_or_default().to_owned());
    let username = text(req, "username").unwrap_or_default();
    json!({ "score": strength::score(&password, username) })
}

/// Janela deslizante de preenchimentos. / Sliding window of fills.
pub struct FillLimiter(Vec<Instant>);

impl FillLimiter {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn allow(&mut self, per_minute: usize) -> bool {
        let now = Instant::now();
        self.0.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if self.0.len() >= per_minute {
            return false;
        }
        self.0.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_must_be_full_and_web() {
        assert!(origins(&json!({ "origin": "https://banco.example", "top": "https://banco.example" })).is_some());
        assert!(origins(&json!({ "origin": "banco.example", "top": "https://banco.example" })).is_none());
        assert!(origins(&json!({ "origin": "chrome-extension://abc", "top": "https://x.example" })).is_none());
        assert!(origins(&json!({ "origin": "https://a.example" })).is_none());
    }

    #[test]
    fn limiter_caps_per_minute() {
        let mut l = FillLimiter::new();
        assert!((0..3).all(|_| l.allow(3)));
        assert!(!l.allow(3));
    }
}
