//! Regra de origem: decide em qual página uma senha pode ser preenchida. Endereços vêm do navegador
//! e de itens salvos; nada pode dar pânico, e a regra precisa ser simétrica com ela mesma.
//! Origin rule: decides on which page a password may be filled. Addresses come from the browser and
//! saved items; nothing may panic, and the rule must be consistent with itself.
#![no_main]
use libfuzzer_sys::fuzz_target;
use sigilvault_core::Origin;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let (a, b) = text.split_once('\n').unwrap_or((text, text));
    let (Some(saved), Some(page)) = (Origin::parse(a), Origin::parse(b)) else { return };
    let _ = (saved.host(), saved.is_secure(), saved.to_canonical());
    // Uma origem sempre serve para ela mesma. / An origin always fits itself.
    assert!(saved.fills(&saved, false));
    // Nunca preenche uma página http com senha de https. / Never fills an http page with an https password.
    if saved.is_secure() && !page.is_secure() {
        assert!(!saved.fills(&page, true));
        assert!(!saved.fills(&page, false));
    }
});
