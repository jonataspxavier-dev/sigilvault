//! Assinatura de executáveis. O Linux não tem Authenticode, então todo executável é "sem
//! assinatura", e a confiança vem de [`crate::protected`] (instalação em pasta do root).
//! Executable signatures. Linux has no Authenticode, so every executable is "unsigned", and trust
//! comes from [`crate::protected`] (install in a root-owned folder).

use std::path::Path;

/// Resultado da checagem. `Trusted` nunca ocorre no Linux, mas mantém o tipo igual ao do Windows.
/// Check result. `Trusted` never occurs on Linux, but keeps the type the same as on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signer {
    Unsigned,
    Trusted([u8; 32]),
    Untrusted,
}

/// Quem assinou o executável. No Linux, ninguém. / Who signed the executable. On Linux, nobody.
pub fn signer(_path: &Path) -> Signer {
    Signer::Unsigned
}

/// Um lado aceita o outro? Versão de depuração aceita; versão final só aceita os dois lado a lado
/// numa pasta protegida (instalação do pacote .deb ou .rpm) ou, no AppImage, o conector idêntico ao empacotado.
/// Does one side accept the other? Debug builds accept; release builds only accept the two side by
/// side in a protected folder (the .deb or .rpm package install).
pub fn peer_allowed(own: &Path, peer: &Path) -> bool {
    cfg!(debug_assertions) || crate::protected::protected_install(own, peer) || appimage_host(own, peer)
}

/// Modo AppImage: o conector que fala com o app tem o mesmo conteúdo do que veio no AppImage.
/// AppImage mode: the connector talking to the app has the same content as the one that came in the AppImage.
fn appimage_host(own: &Path, peer: &Path) -> bool {
    crate::appimage::running_appimage().is_some()
        && own.parent().map(|dir| dir.join(crate::HOST_EXE)).is_some_and(|bundled| crate::appimage::same_content(&bundled, peer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_unsigned() {
        assert_eq!(signer(Path::new("/bin/sh")), Signer::Unsigned);
        assert_eq!(signer(Path::new("/nao/existe")), Signer::Unsigned);
    }
}
