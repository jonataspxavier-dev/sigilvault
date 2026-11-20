//! Diálogos nativos do Windows (confirmação, abrir e salvar). Eles são abertos pelo Rust: a
//! interface nunca escolhe caminhos de arquivo nem pula confirmações.
//! Native Windows dialogs (confirm, open and save). They are opened by Rust: the UI never picks
//! file paths nor skips confirmations.

use std::path::{Path, PathBuf};

use tauri::WebviewWindow;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Texto no idioma escolhido. / Text in the chosen language.
pub fn tr(lang: &str, pt: &str, en: &str) -> String {
    if lang == "en" { en } else { pt }.to_owned()
}

/// Pergunta de confirmação com aviso. Devolve `true` se o usuário confirmou.
/// Confirmation question with a warning. Returns `true` if the user confirmed.
pub fn confirm(window: &WebviewWindow, lang: &str, title: &str, text: &str, ok: &str) -> bool {
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
