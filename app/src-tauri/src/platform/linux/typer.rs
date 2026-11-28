//! Auto-Type: digitar em outro programa. Ainda não implementado no Linux: sempre recusa.
//! Auto-Type: typing into another program. Not implemented on Linux yet: always refuses.

use zeroize::Zeroizing;

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

/// Digita os blocos no programa-alvo. / Types the blocks into the target program.
pub fn type_into(_target: &Target, _parts: &[Part]) -> Result<(), TypeError> {
    Err(TypeError::Blocked)
}
