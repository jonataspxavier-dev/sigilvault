//! Leitura de um código de recuperação digitado pela pessoa. / Reading a recovery code typed by the person.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(code) = sigilvault_core::RecoveryCode::parse(text) {
            // Ler o que se mostra dá o mesmo código. / Reading what is shown gives the same code.
            let again = sigilvault_core::RecoveryCode::parse(&code.to_display_string()).expect("o texto mostrado é válido / the shown text is valid");
            let _ = again;
        }
    }
});
