//! Estrutura do arquivo `.sigil` (cabeçalho, limites, parâmetros do Argon2), sem abrir com senha.
//! `.sigil` file structure (header, limits, Argon2 parameters), without opening with a password.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = sigilvault_core::Vault::check_file(data);
});
