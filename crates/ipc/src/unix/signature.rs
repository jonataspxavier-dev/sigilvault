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
/// numa pasta protegida (instalação do pacote .deb ou .rpm).
/// Does one side accept the other? Debug builds accept; release builds only accept the two side by
/// side in a protected folder (the .deb or .rpm package install).
pub fn peer_allowed(own: &Path, peer: &Path) -> bool {
    cfg!(debug_assertions) || crate::protected::protected_install(own, peer)
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
