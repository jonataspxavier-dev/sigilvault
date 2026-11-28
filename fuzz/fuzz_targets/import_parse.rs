//! Importadores (CSV e XML de outros gerenciadores): arquivo de fora, tratado como hostil.
//! Importers (CSV and XML from other managers): outside file, treated as hostile.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(parsed) = sigilvault_core::import::parse(data) {
        // Nunca mais itens do que bytes. / Never more items than bytes.
        assert!(parsed.items.len() <= data.len().max(1));
    }
});
