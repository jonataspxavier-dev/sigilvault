//! Atalhos globais. Ainda não implementados no Linux: `apply` diz que nenhum foi aceito.
//! O texto da combinação já é lido e normalizado, para as configurações funcionarem.
//! Global hotkeys. Not implemented on Linux yet: `apply` says none was accepted. The combination
//! text is already parsed and normalized, so the settings work.

use super::target::Target;

/// O que cada atalho faz. / What each hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Pop-up de busca (copiar ou digitar). / Search popup (copy or type).
    Search = 1,
    /// "Gerar ou só salvar" para programas. / "Generate or just save" for programs.
    Generate = 2,
}

const CTRL: u8 = 1;
const ALT: u8 = 2;
const SHIFT: u8 = 4;
const SUPER: u8 = 8;

/// Combinação de teclas. / Key combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    mods: u8,
    /// Nome canônico da tecla: `Space`, `F5`, `K`, `7`. / Canonical key name.
    key: String,
}

/// Lê "Ctrl+Shift+Espaço", "Alt+F2", "Super+K"… Exige Ctrl, Alt ou Super (Shift sozinho roubaria
/// letras maiúsculas). "Win" e "Meta" valem como Super.
/// Reads "Ctrl+Shift+Space", "Alt+F2", "Super+K"… Requires Ctrl, Alt or Super (Shift alone would
/// steal capital letters). "Win" and "Meta" count as Super.
pub fn parse(text: &str) -> Option<Combo> {
    let mut mods = 0u8;
    let mut key = None;
    for raw in text.split('+') {
        let part = raw.trim().to_lowercase();
        match part.as_str() {
            "ctrl" | "control" => mods |= CTRL,
            "alt" => mods |= ALT,
            "shift" => mods |= SHIFT,
            "super" | "win" | "meta" => mods |= SUPER,
            "space" | "espaço" | "espaco" => key = Some("Space".to_owned()),
            p if p.len() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric()) => key = Some(p.to_ascii_uppercase()),
            p if p.starts_with('f') => match p[1..].parse::<u32>() {
                Ok(n @ 1..=24) => key = Some(format!("F{n}")),
                _ => return None,
            },
            _ => return None,
        }
    }
    let key = key?;
    if mods & (CTRL | ALT | SUPER) == 0 {
        return None;
    }
    Some(Combo { mods, key })
}

/// Texto canônico da combinação. / Canonical text of the combination.
pub fn format(c: Combo) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (flag, name) in [(CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift"), (SUPER, "Super")] {
        if c.mods & flag != 0 {
            parts.push(name);
        }
    }
    parts.push(&c.key);
    parts.join("+")
}

/// Inicia a escuta dos atalhos. `on_press` recebe a ação e a janela que estava ativa.
/// Starts listening for hotkeys. `on_press` gets the action and the window that was active.
pub fn start(_on_press: impl Fn(Action, Option<Target>) + Send + Sync + 'static) {}

/// Troca os atalhos. Devolve, para cada um, se foi aceito. `None` desliga o atalho.
/// Replaces the hotkeys. Returns, for each one, whether it was accepted. `None` turns it off.
pub fn apply(bindings: Vec<(Action, Option<Combo>)>) -> Vec<(Action, bool)> {
    bindings.into_iter().map(|(a, c)| (a, c.is_none())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_combinations() {
        assert_eq!(format(parse("ctrl + shift + espaço").unwrap()), "Ctrl+Shift+Space");
        assert_eq!(format(parse("Alt+F2").unwrap()), "Alt+F2");
        assert_eq!(format(parse("Ctrl+Alt+k").unwrap()), "Ctrl+Alt+K");
        assert_eq!(format(parse("Win+7").unwrap()), "Super+7");
        for bad in ["Shift+A", "A", "Ctrl+Shift+Enter", "Ctrl+F25", "Ctrl+Shift", "", "Ctrl+AB"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }
}
