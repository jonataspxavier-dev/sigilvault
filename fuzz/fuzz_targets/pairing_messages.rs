//! Mensagens do pareamento que chegam da extensão (OPAQUE) e do canal criptografado: qualquer
//! lixo precisa virar um erro, nunca um pânico.
//! Pairing messages that arrive from the extension (OPAQUE) and the encrypted channel: any junk
//! must become an error, never a panic.
#![no_main]
use libfuzzer_sys::fuzz_target;
use sigilvault_pairing::{Channel, FillRng, PairingCode, Role, ServerPairing};

/// Acaso fixo: o fuzzing precisa ser repetível. (Só aqui, nunca no app.) / Fixed randomness: fuzzing must be repeatable. (Only here, never in the app.)
fn rng() -> FillRng<impl FnMut(&mut [u8])> {
    let mut counter = 0u8;
    FillRng(move |buf: &mut [u8]| {
        for b in buf {
            counter = counter.wrapping_mul(31).wrapping_add(7);
            *b = counter;
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let (first, rest) = data.split_at(data.len() / 2);
    // 1. Primeiro e segundo passo do login do lado do app. / First and second step of the app-side login.
    let code = PairingCode::generate(&mut |buf: &mut [u8]| buf.iter_mut().enumerate().for_each(|(i, b)| *b = i as u8 * 13 + 5));
    let mut r = rng();
    if let Ok(server) = ServerPairing::new(&mut r, &code) {
        if let Ok((login, _response)) = server.login_start(&mut r, first) {
            let _ = login.finish(rest);
        }
    }
    // 2. Canal criptografado: lixo não abre. / Encrypted channel: junk does not open.
    let mut channel = Channel::new(&[1; 32], &[2; 32], &[3; 32], Role::App);
    assert!(channel.open(data).is_err() || data.len() >= 16);
});
