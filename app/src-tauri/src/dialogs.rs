//! Diálogos nativos do Windows (confirmação, abrir e salvar). Eles são abertos pelo Rust: a
//! interface nunca escolhe caminhos de arquivo nem pula confirmações.
//! Native Windows dialogs (confirm, open and save). They are opened by Rust: the UI never picks
//! file paths nor skips confirmations.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

/// Só em builds de depuração: `SIGILVAULT_TEST_DIALOG=<arquivo de controle>` faz as confirmações
/// responderem "sim" e os diálogos de arquivo devolverem o caminho escrito nesse arquivo (para
/// testes automáticos). Em release, esta função nem existe.
/// Debug builds only: `SIGILVAULT_TEST_DIALOG=<control file>` makes confirmations answer "yes" and
/// file dialogs return the path written in that file (for automated tests). In release this
/// function does not exist.
#[cfg(debug_assertions)]
fn test_answer() -> Option<PathBuf> {
    let control = std::env::var_os("SIGILVAULT_TEST_DIALOG")?;
    let path = std::fs::read_to_string(control).ok()?;
    Some(PathBuf::from(path.trim()))
}

/// Só em depuração: o texto do arquivo de controle (para respostas como "always"/"once"/"cancel").
/// Debug only: the control file's text (for answers like "always"/"once"/"cancel").
#[cfg(debug_assertions)]
fn test_text() -> Option<String> {
    let control = std::env::var_os("SIGILVAULT_TEST_DIALOG")?;
    std::fs::read_to_string(control).ok().map(|s| s.trim().to_owned())
}

/// Resposta ao pedido de preenchimento. / Answer to a fill request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillChoice {
    /// Permitir sempre neste site (neste navegador). / Allow always on this site (in this browser).
    Always,
    /// Só desta vez. / Just this once.
    Once,
    Cancel,
}

/// Pergunta se a extensão pode preencher. Abre a janela do app para o diálogo não ficar escondido
/// atrás do navegador. Com `allow_always = false` (formulário dentro de outro site), só há
/// "desta vez". / Asks whether the extension may fill. Opens the app window so the dialog is not
/// hidden behind the browser. With `allow_always = false` (form inside another site), only "this once".
pub fn ask_fill(app: &AppHandle, lang: &str, text: &str, allow_always: bool) -> FillChoice {
    #[cfg(debug_assertions)]
    if let Some(answer) = test_text() {
        return match answer.as_str() {
            "always" if allow_always => FillChoice::Always,
            "cancel" => FillChoice::Cancel,
            _ => FillChoice::Once,
        };
    }
    crate::window::open(app);
    let always = tr(lang, "Permitir sempre neste site", "Always allow on this site");
    let once = tr(lang, "Só desta vez", "Just this once");
    let cancel = tr(lang, "Cancelar", "Cancel");
    let mut builder = app.dialog().message(text).title(tr(lang, "SigilVault — preencher senha?", "SigilVault — fill password?")).kind(MessageDialogKind::Warning);
    if let Some(w) = crate::window::current(app) {
        builder = builder.parent(&w);
    }
    builder = if allow_always {
        builder.buttons(MessageDialogButtons::YesNoCancelCustom(always.clone(), once.clone(), cancel))
    } else {
        builder.buttons(MessageDialogButtons::OkCancelCustom(once.clone(), cancel))
    };
    match builder.blocking_show_with_result() {
        MessageDialogResult::Yes if allow_always => FillChoice::Always,
        MessageDialogResult::Ok | MessageDialogResult::No => FillChoice::Once,
        MessageDialogResult::Yes => FillChoice::Once,
        MessageDialogResult::Custom(label) if allow_always && label == always => FillChoice::Always,
        MessageDialogResult::Custom(label) if label == once => FillChoice::Once,
        _ => FillChoice::Cancel,
    }
}

/// Texto no idioma escolhido. / Text in the chosen language.
pub fn tr(lang: &str, pt: &str, en: &str) -> String {
    if lang == "en" { en } else { pt }.to_owned()
}

/// Pergunta de confirmação com aviso. Devolve `true` se o usuário confirmou.
/// Confirmation question with a warning. Returns `true` if the user confirmed.
pub fn confirm(window: &WebviewWindow, lang: &str, title: &str, text: &str, ok: &str) -> bool {
    #[cfg(debug_assertions)]
    if test_answer().is_some() {
        return true;
    }
    window
        .dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(ok.to_owned(), tr(lang, "Cancelar", "Cancel")))
        .parent(window)
        .blocking_show()
}

/// "Salvar como". / "Save as".
pub fn save_path(window: &WebviewWindow, title: &str, file_name: &str, filter: (&str, &[&str])) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(p) = test_answer() {
        return Some(p);
    }
    window
        .dialog()
        .file()
        .set_parent(window)
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(filter.0, filter.1)
        .blocking_save_file()?
        .into_path()
        .ok()
}

/// "Abrir". / "Open".
pub fn pick_path(window: &WebviewWindow, title: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(p) = test_answer() {
        return Some(p);
    }
    let mut builder = window.dialog().file().set_parent(window).set_title(title);
    for (name, ext) in filters {
        builder = builder.add_filter(*name, ext);
    }
    builder.blocking_pick_file()?.into_path().ok()
}

/// O caminho está numa pasta sincronizada com a nuvem (OneDrive, Dropbox, Google Drive, iCloud)?
/// Is the path in a cloud-synced folder (OneDrive, Dropbox, Google Drive, iCloud)?
pub fn is_cloud_folder(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase();
    let from_env = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().any(|var| {
        std::env::var_os(var).is_some_and(|v| !v.is_empty() && lower.starts_with(&v.to_string_lossy().to_lowercase()))
    });
    from_env
        || path.components().any(|c| {
            let name = c.as_os_str().to_string_lossy().to_lowercase();
            name.starts_with("onedrive") || name == "dropbox" || name == "google drive" || name == "icloud drive" || name == "icloudrive"
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cloud_folders() {
        assert!(is_cloud_folder(Path::new(r"C:\Users\ana\OneDrive\Documentos\senhas.csv")));
        assert!(is_cloud_folder(Path::new(r"C:\Users\ana\OneDrive - Empresa\x.csv")));
        assert!(is_cloud_folder(Path::new(r"D:\Dropbox\x.csv")));
        assert!(is_cloud_folder(Path::new(r"G:\Google Drive\x.csv")));
        assert!(!is_cloud_folder(Path::new(r"C:\Users\ana\Desktop\x.csv")));
        assert!(!is_cloud_folder(Path::new(r"E:\pendrive\x.csv")));
    }
}
