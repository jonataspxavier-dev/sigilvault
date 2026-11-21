//! Preferências do usuário (não secretas).
//! User preferences (not secret).

use std::path::Path;

use serde::{Deserialize, Serialize};
use sigilvault_core::GeneratorOptions;

use crate::storage::atomic_write;

/// Opções preferidas do gerador, salvas como padrão.
/// Preferred generator options, saved as default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneratorPrefs {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for GeneratorPrefs {
    fn default() -> Self {
        GeneratorOptions::default().into()
    }
}

impl From<GeneratorOptions> for GeneratorPrefs {
    fn from(o: GeneratorOptions) -> Self {
        Self {
            length: o.length,
            lowercase: o.lowercase,
            uppercase: o.uppercase,
            digits: o.digits,
            symbols: o.symbols,
            exclude_ambiguous: o.exclude_ambiguous,
        }
    }
}

impl From<GeneratorPrefs> for GeneratorOptions {
    fn from(p: GeneratorPrefs) -> Self {
        Self {
            length: p.length,
            lowercase: p.lowercase,
            uppercase: p.uppercase,
            digits: p.digits,
            symbols: p.symbols,
            exclude_ambiguous: p.exclude_ambiguous,
        }
    }
}

/// Preferências do app. / App preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "pt" ou "en". / "pt" or "en".
    pub language: String,
    /// Minutos sem usar o PC até bloquear. / Minutes of PC inactivity before locking.
    pub auto_lock_minutes: u32,
    /// Segundos até limpar a área de transferência. / Seconds until the clipboard is cleared.
    pub clipboard_clear_secs: u32,
    /// Padrão do gerador. / Generator defaults.
    pub generator: GeneratorPrefs,
    /// Atalho do pop-up de busca (vazio = desligado). / Search popup hotkey (empty = off).
    pub hotkey_search: String,
    /// Atalho de gerar (vazio = desligado). / Generate hotkey (empty = off).
    pub hotkey_generate: String,
    /// Abrir na bandeja ao entrar no Windows (cofre bloqueado). / Open in the tray at Windows sign-in (vault locked).
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "pt".into(),
            auto_lock_minutes: 5,
            clipboard_clear_secs: 30,
            generator: GeneratorPrefs::default(),
            hotkey_search: "Ctrl+Shift+Space".into(),
            hotkey_generate: "Ctrl+Shift+G".into(),
            start_with_windows: false,
        }
    }
}

impl Settings {
    /// Corrige valores fora dos limites (arquivo editado à mão ou interface com bug).
    /// Fixes out-of-range values (hand-edited file or buggy UI).
    pub fn sanitized(mut self) -> Self {
        if self.language != "pt" && self.language != "en" {
            self.language = "pt".into();
        }
        self.auto_lock_minutes = self.auto_lock_minutes.clamp(1, 120);
        // Atalho inválido volta ao padrão; vazio fica desligado; válido fica no formato canônico.
        // Invalid hotkey goes back to default; empty stays off; valid is stored in canonical form.
        let defaults = Self::default();
        for (value, default) in [(&mut self.hotkey_search, defaults.hotkey_search), (&mut self.hotkey_generate, defaults.hotkey_generate)] {
            if value.trim().is_empty() {
                value.clear();
            } else {
                *value = crate::win::hotkeys::parse(value).map(crate::win::hotkeys::format).unwrap_or(default);
            }
        }
        self.clipboard_clear_secs = self.clipboard_clear_secs.clamp(10, 120);
        self.generator.length = self.generator.length.clamp(4, sigilvault_core::generator::MAX_LENGTH);
        let g = &self.generator;
        if !(g.lowercase || g.uppercase || g.digits || g.symbols) {
            self.generator = GeneratorPrefs { length: self.generator.length, ..GeneratorPrefs::default() };
        }
        self
    }

    /// Lê do disco; se não existir ou estiver ilegível, usa o padrão.
    /// Reads from disk; falls back to defaults if missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Grava no disco. / Writes to disk.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        atomic_write(path, &serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_out_of_range_values() {
        let s = Settings {
            language: "xx".into(),
            auto_lock_minutes: 0,
            clipboard_clear_secs: 999,
            generator: GeneratorPrefs { length: 9999, lowercase: false, uppercase: false, digits: false, symbols: false, exclude_ambiguous: true },
            hotkey_search: "shift+a".into(),
            hotkey_generate: "  ".into(),
            start_with_windows: true,
        }
        .sanitized();
        assert_eq!(s.hotkey_search, "Ctrl+Shift+Space");
        assert_eq!(s.hotkey_generate, "");
        assert_eq!(Settings { hotkey_search: "alt + k".into(), ..Settings::default() }.sanitized().hotkey_search, "Alt+K");
        assert_eq!(s.language, "pt");
        assert_eq!(s.auto_lock_minutes, 1);
        assert_eq!(s.clipboard_clear_secs, 120);
        assert_eq!(s.generator.length, 128);
        assert!(s.generator.lowercase);
    }

    #[test]
    fn save_and_load() {
        let dir = crate::storage::test_dir("settings");
        let path = dir.join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        let s = Settings { language: "en".into(), auto_lock_minutes: 10, ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, b"{lixo").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
