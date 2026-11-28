//! Selo nos campos de senha de outros programas. Ainda não implementado no Linux: `start` não
//! faz nada, então nenhum selo aparece. Os tipos já existem para o resto do app compilar.
//! Seal on other programs' password fields. Not implemented on Linux yet: `start` does nothing,
//! so no seal appears. The types already exist so the rest of the app compiles.

use std::path::{Path, PathBuf};

use super::target::Target;

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

/// Ícone do selo em RGBA (de cima para baixo). / Seal icon in RGBA (top to bottom).
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Começa a vigiar os campos de senha. / Starts watching password fields.
pub fn start(_icon: Icon, _policy: impl Fn(&Path) -> Policy + Send + 'static, _on_event: impl Fn(Event) + Send + 'static) {}

/// Põe o foco num campo do programa. / Focuses a field of the program.
pub fn focus(_token: u64, _slot: Slot) -> bool {
    false
}

/// Marca o campo como preenchido por nós. / Marks the field as filled by us.
pub fn mark_filled(_token: u64) {}

/// Esconde o selo. / Hides the seal.
pub fn hide_seal() {}

/// Textos do menu do selo. / Seal menu labels.
pub fn set_menu_labels(_open: &str, _ignore: &str) {}

/// Anotação de depuração. Sem efeito no Linux por enquanto. / Debug note. No effect on Linux for now.
pub fn trace(_msg: &str) {}
