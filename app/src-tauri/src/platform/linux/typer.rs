//! Auto-Type: escrever em outro programa, pela acessibilidade.
//! Auto-Type: writing into another program, through accessibility.

use zeroize::Zeroizing;

use super::atspi::Element;
use super::target::Target;

/// Um bloco de digitação. / One typing block.
pub enum Part {
    Text(Zeroizing<String>),
    Tab,
    /// Ctrl+A: o texto seguinte substitui o que já estava no campo. / Ctrl+A: the next text replaces what was in the field.
    SelectAll,
    /// Põe o foco num campo do programa; `false` = não deu. / Focuses a field of the program; `false` = it failed.
    Focus(Box<dyn Fn() -> bool + Send + Sync>),
}

/// Por que a digitação não aconteceu (ou parou). / Why typing did not happen (or stopped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeError {
    /// O usuário continuou segurando Ctrl/Shift/Alt. / The user kept holding Ctrl/Shift/Alt.
    KeysHeld,
    /// A janela mudou. / The window changed.
    WindowChanged,
    /// O sistema recusou. / The system refused.
    Blocked,
}

/// O campo em que o próximo texto vai. Definido por `fields::focus` ou, sem ele, pelo último foco.
/// The field the next text goes into. Set by `fields::focus` or, without it, by the last focus.
static CURRENT: std::sync::Mutex<Option<Element>> = std::sync::Mutex::new(None);

/// Guarda o campo atual. / Stores the current field.
pub(super) fn set_current(el: Option<Element>) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = el;
}

fn current() -> Option<Element> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Digita os blocos no programa-alvo. No Linux isso é escrever no campo pela acessibilidade
/// (`EditableText`), sem simular teclas: funciona no Wayland, mas só em programas que expõem o campo.
/// Types the blocks into the target program. On Linux this is writing into the field through
/// accessibility (`EditableText`), without simulating keys: it works on Wayland, but only in
/// programs that expose the field.
pub fn type_into(target: &Target, parts: &[Part]) -> Result<(), TypeError> {
    // Parte do campo que estava com o foco no alvo, a menos que `Focus` diga outro.
    // Starts from the field that had focus in the target, unless `Focus` says another.
    set_current(super::atspi::last_focus().filter(|f| f.pid == target.pid).map(|f| f.element));
    let result = run(target, parts);
    // Não deixa o campo de uma digitação valer para a próxima. / Does not let one typing's field carry into the next.
    set_current(None);
    result
}

fn run(target: &Target, parts: &[Part]) -> Result<(), TypeError> {
    for part in parts {
        match part {
            Part::Focus(focus) => {
                if !focus() {
                    return Err(TypeError::Blocked);
                }
            }
            Part::SelectAll => {}
            Part::Tab => {
                let next = current().and_then(|el| super::atspi::next_sibling(&el)).ok_or(TypeError::Blocked)?;
                if !super::atspi::grab_focus(&next) {
                    return Err(TypeError::Blocked);
                }
                set_current(Some(next));
            }
            Part::Text(text) => {
                // Se outro programa ganhou o foco no meio do caminho, para. / If another program got focus midway, stop.
                if super::atspi::last_focus().is_some_and(|f| f.pid != target.pid) {
                    return Err(TypeError::WindowChanged);
                }
                let el = current().ok_or(TypeError::Blocked)?;
                if !super::atspi::set_text(&el, text) {
                    return Err(TypeError::Blocked);
                }
            }
        }
    }
    Ok(())
}
