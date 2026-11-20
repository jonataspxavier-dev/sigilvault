//! Exportar, importar e backups. Os caminhos de arquivo vêm sempre de diálogos nativos abertos
//! aqui no Rust; a interface só pede a ação. A trava do estado nunca fica presa enquanto um
//! diálogo está aberto (o bloqueio automático precisa poder agir).
//! Export, import and backups. File paths always come from native dialogs opened here in Rust;
//! the UI only asks for the action. The state lock is never held while a dialog is open
//! (auto-lock must be able to act).

use std::fs;
use std::io::Write;

use serde::Serialize;
use sigilvault_core::import::{self, Format};
use sigilvault_core::{KdfParams, Vault};
use tauri::{State, WebviewWindow};
use zeroize::Zeroizing;

use crate::commands::{CmdError, CmdResult, backup_before, persist, vault_mut, vault_ref};
use crate::dialogs::{self, tr};
use crate::state::{AppState, Profile};
use crate::storage::atomic_write;
use crate::backups;

const SIGIL_FILTER: (&str, &[&str]) = ("SigilVault", &["sigil"]);

fn language(state: &AppState) -> String {
    state.lock().settings.language.clone()
}

/// Lê um arquivo escolhido pelo usuário, com limite de tamanho. / Reads a user-picked file, with a size limit.
fn read_limited(path: &std::path::Path, limit: usize) -> CmdResult<Zeroizing<Vec<u8>>> {
    if fs::metadata(path)?.len() > limit as u64 {
        return Err(CmdError::new("file_too_large"));
    }
    Ok(Zeroizing::new(fs::read(path)?))
}

// ── Exportar / Export ─────────────────────────────────────────────

/// Exporta o cofre. `mode`: "copy" (cópia exata, abre com a senha mestra ou um código) ou
/// "password" (cópia com senha própria, sem códigos). Devolve `false` se o usuário cancelou.
/// Exports the vault. `mode`: "copy" (exact copy, opens with the master password or a code) or
/// "password" (copy with its own password, no codes). Returns `false` if the user cancelled.
#[tauri::command(async)]
pub fn export_vault(window: WebviewWindow, state: State<'_, AppState>, mode: String, password: Option<String>) -> CmdResult<bool> {
    let password = password.map(Zeroizing::new);
    let lang = language(&state);
    // Com senha própria: gera a cópia antes, para recusar senha fraca sem abrir diálogos.
    // With its own password: build the copy first, to reject a weak password without dialogs.
    let prepared: Option<Vec<u8>> = match mode.as_str() {
        "copy" => {
            vault_ref(&state.lock())?;
            None
        }
        "password" => {
            let pw = password.as_ref().ok_or(CmdError::new("invalid_input"))?;
            Some(vault_ref(&state.lock())?.export_with_password(pw, KdfParams::DEFAULT)?)
        }
        _ => return Err(CmdError::new("invalid_input")),
    };

    let text = tr(
        &lang,
        "A cópia exportada fica criptografada, mas não pode ser revogada: quem tiver o arquivo e a senha (ou, na cópia simples, um código de recuperação) poderá abri-la para sempre, mesmo que você troque a senha depois.\n\nGuarde-a em um lugar seguro.",
        "The exported copy stays encrypted but cannot be revoked: whoever has the file and the password (or, for a plain copy, a recovery code) can open it forever, even if you change the password later.\n\nKeep it in a safe place.",
    );
    if !dialogs::confirm(&window, &lang, &tr(&lang, "Exportar o cofre", "Export the vault"), &text, &tr(&lang, "Exportar", "Export")) {
        return Ok(false);
    }
    let Some(dest) = dialogs::save_path(&window, &tr(&lang, "Salvar cópia do cofre", "Save vault copy"), "cofre-exportado.sigil", SIGIL_FILTER) else {
        return Ok(false);
    };
    if dest == state.paths.vault {
        return Err(CmdError::new("invalid_input"));
    }
    let bytes = match prepared {
        Some(b) => b,
        None => {
            // A cópia simples é o próprio arquivo, já salvo. / The plain copy is the file itself, already saved.
            vault_ref(&state.lock())?;
            fs::read(&state.paths.vault)?
        }
    };
    atomic_write(&dest, &bytes)?;
    Ok(true)
}

/// Exporta CSV legível: pede a senha mestra, mostra aviso forte e avisa sobre pastas na nuvem.
/// Exports readable CSV: asks for the master password, shows a strong warning and warns about cloud folders.
#[tauri::command(async)]
pub fn export_csv(window: WebviewWindow, state: State<'_, AppState>, master_password: String) -> CmdResult<bool> {
    let master_password = Zeroizing::new(master_password);
    let lang = language(&state);
    vault_ref(&state.lock())?.verify_master_password(&master_password)?;

    let text = tr(
        &lang,
        "ATENÇÃO: o CSV NÃO é criptografado. Qualquer pessoa ou programa com acesso ao arquivo verá todas as suas senhas.\n\nUse só para migrar para outro gerenciador e apague o arquivo logo depois.",
        "WARNING: the CSV is NOT encrypted. Anyone or any program with access to the file will see all your passwords.\n\nUse it only to migrate to another manager and delete the file right afterwards.",
    );
    if !dialogs::confirm(&window, &lang, &tr(&lang, "Exportar senhas em texto puro", "Export passwords as plain text"), &text, &tr(&lang, "Entendo, exportar", "I understand, export")) {
        return Ok(false);
    }
    let Some(dest) = dialogs::save_path(&window, &tr(&lang, "Salvar CSV", "Save CSV"), "sigilvault-senhas.csv", ("CSV", &["csv"])) else {
        return Ok(false);
    };
    if dialogs::is_cloud_folder(&dest) {
        let cloud = tr(
            &lang,
            "Esta pasta é sincronizada com a nuvem (OneDrive, Dropbox ou similar). O CSV com todas as senhas em texto puro seria enviado para lá.\n\nRecomendado: cancele e escolha uma pasta local ou um pendrive.",
            "This folder syncs to the cloud (OneDrive, Dropbox or similar). The CSV with every password in plain text would be uploaded there.\n\nRecommended: cancel and pick a local folder or a USB drive.",
        );
        if !dialogs::confirm(&window, &lang, &tr(&lang, "Pasta na nuvem", "Cloud folder"), &cloud, &tr(&lang, "Salvar mesmo assim", "Save anyway")) {
            return Ok(false);
        }
    }
    let csv = vault_ref(&state.lock())?.export_csv()?;
    atomic_write(&dest, &csv)?;
    Ok(true)
}

// ── Importar / Import ─────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDto {
    format: &'static str,
    added: usize,
    duplicates: usize,
    skipped: usize,
    /// O arquivo era texto puro (vale oferecer apagá-lo). / The file was plain text (worth offering deletion).
    plaintext: bool,
}

fn format_name(f: Format) -> &'static str {
    match f {
        Format::SigilVaultCsv => "SigilVault (CSV)",
        Format::ChromeCsv => "Chrome / Edge / Brave / Opera (CSV)",
        Format::FirefoxCsv => "Firefox (CSV)",
        Format::BitwardenCsv => "Bitwarden (CSV)",
        Format::BitwardenJson => "Bitwarden (JSON)",
        Format::KeePassXml => "KeePass 2 (XML)",
        Format::KeePassXcCsv => "KeePassXC (CSV)",
        Format::GenericCsv => "CSV",
    }
}

/// Importa de outro gerenciador (Chrome, Firefox, Bitwarden, KeePass). `None` = cancelado.
/// Imports from another manager (Chrome, Firefox, Bitwarden, KeePass). `None` = cancelled.
#[tauri::command(async)]
pub fn import_file(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<Option<ImportDto>> {
    let lang = language(&state);
    vault_ref(&state.lock())?;
    let filters: [(&str, &[&str]); 2] = [
        (&tr(&lang, "Exportações (CSV, JSON, XML)", "Exports (CSV, JSON, XML)"), &["csv", "json", "xml"]),
        (&tr(&lang, "Todos os arquivos", "All files"), &["*"]),
    ];
    let Some(path) = dialogs::pick_path(&window, &tr(&lang, "Importar senhas", "Import passwords"), &filters) else {
        return Ok(None);
    };
    let bytes = read_limited(&path, import::MAX_IMPORT_LEN)?;
    if bytes.starts_with(b"SIGILVLT") {
        return Err(CmdError::new("use_merge"));
    }
    let parsed = import::parse(&bytes)?;

    let mut inner = state.lock();
    vault_ref(&inner)?;
    backup_before(&mut inner, &state, true);
    let report = vault_mut(&mut inner)?.import_items(&parsed.items)?;
    persist(&mut inner, &state)?;
    inner.last_import = Some(path);
    Ok(Some(ImportDto {
        format: format_name(parsed.format),
        added: report.added,
        duplicates: report.duplicates,
        skipped: parsed.skipped,
        plaintext: true,
    }))
}

/// Apaga o último arquivo importado (sobrescreve com zeros antes; em SSD isso não garante que
/// os dados sumam do disco, mas ajuda).
/// Deletes the last imported file (overwrites with zeros first; on SSDs this does not guarantee
/// the data is gone from disk, but it helps).
#[tauri::command(async)]
pub fn delete_imported_file(state: State<'_, AppState>) -> CmdResult<bool> {
    let Some(path) = state.lock().last_import.take() else { return Ok(false) };
    if let Ok(len) = fs::metadata(&path).map(|m| m.len())
        && let Ok(mut f) = fs::OpenOptions::new().write(true).open(&path)
    {
        let zeros = vec![0u8; 64 * 1024];
        let mut left = len;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            if f.write_all(&zeros[..n]).is_err() {
                break;
            }
            left -= n as u64;
        }
        let _ = f.sync_all();
    }
    fs::remove_file(&path)?;
    Ok(true)
}

/// Traz os itens de outro arquivo `.sigil` (ex.: uma cópia com senha própria). `None` = cancelado.
/// Brings in items from another `.sigil` file (e.g. a copy with its own password). `None` = cancelled.
#[tauri::command(async)]
pub fn merge_vault(window: WebviewWindow, state: State<'_, AppState>, password: String) -> CmdResult<Option<ImportDto>> {
    let password = Zeroizing::new(password);
    let lang = language(&state);
    vault_ref(&state.lock())?;
    let Some(path) = dialogs::pick_path(&window, &tr(&lang, "Importar de outro cofre", "Import from another vault"), &[SIGIL_FILTER]) else {
        return Ok(None);
    };
    let bytes = read_limited(&path, 256 * 1024 * 1024)?;
    let other = Vault::open(&bytes, &password)?;

    let mut inner = state.lock();
    vault_ref(&inner)?;
    backup_before(&mut inner, &state, true);
    let report = vault_mut(&mut inner)?.merge_from(&other)?;
    persist(&mut inner, &state)?;
    Ok(Some(ImportDto { format: "SigilVault", added: report.added, duplicates: report.duplicates, skipped: 0, plaintext: false }))
}

/// Em um PC sem cofre: copia um `.sigil` (exportado em outro PC) para a pasta do app.
/// On a PC without a vault: copies a `.sigil` (exported on another PC) into the app folder.
#[tauri::command(async)]
pub fn import_vault_file(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<bool> {
    let lang = language(&state);
    if state.paths.vault.exists() {
        return Err(CmdError::new("vault_exists"));
    }
    let Some(path) = dialogs::pick_path(&window, &tr(&lang, "Abrir cofre exportado", "Open exported vault"), &[SIGIL_FILTER]) else {
        return Ok(false);
    };
    let bytes = read_limited(&path, 256 * 1024 * 1024)?;
    Vault::check_file(&bytes)?;
    // Confere de novo: nunca sobrescrever um cofre. / Check again: never overwrite a vault.
    if state.paths.vault.exists() {
        return Err(CmdError::new("vault_exists"));
    }
    atomic_write(&state.paths.vault, &bytes)?;
    Profile::remove(&state.paths);
    Ok(true)
}

// ── Backups ───────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    id: String,
    created_at: u64,
    revision: u64,
    item_count: usize,
}

#[tauri::command(async)]
pub fn list_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupDto>> {
    let inner = state.lock();
    let vault = vault_ref(&inner)?;
    Ok(backups::list(&state.paths.backups)
        .into_iter()
        .filter_map(|path| {
            let info = vault.inspect_backup(&fs::read(&path).ok()?).ok()?;
            Some(BackupDto {
                id: path.file_name()?.to_str()?.to_owned(),
                created_at: info.created_at,
                revision: info.revision,
                item_count: info.item_count,
            })
        })
        .collect())
}

/// Restaura um backup (os itens atuais viram backup antes). `false` = cancelado.
/// Restores a backup (current items are backed up first). `false` = cancelled.
#[tauri::command(async)]
pub fn restore_backup(window: WebviewWindow, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    if !backups::is_valid_name(&id) {
        return Err(CmdError::new("not_found"));
    }
    let path = state.paths.backups.join(&id);
    let lang = language(&state);
    let text = tr(
        &lang,
        "Os itens atuais serão trocados pelos deste backup. Antes disso, o estado atual vira um backup novo.",
        "Current items will be replaced by this backup's. Before that, the current state becomes a new backup.",
    );
    if !dialogs::confirm(&window, &lang, &tr(&lang, "Restaurar backup", "Restore backup"), &text, &tr(&lang, "Restaurar", "Restore")) {
        return Ok(false);
    }
    // Lê antes de criar o backup novo: a rotação poderia apagar este arquivo.
    // Read before creating the new backup: rotation could delete this file.
    let bytes = fs::read(&path).map_err(|_| CmdError::new("not_found"))?;
    let mut inner = state.lock();
    vault_ref(&inner)?.inspect_backup(&bytes)?;
    backup_before(&mut inner, &state, true);
    vault_mut(&mut inner)?.restore_backup(&bytes)?;
    persist(&mut inner, &state)?;
    Ok(true)
}
