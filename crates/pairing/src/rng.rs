//! Adaptador de acaso: o chamador fornece a fonte (getrandom no app, crypto.getRandomValues na
//! extensão). Assim o crate não depende de nenhuma fonte específica.
//! Randomness adapter: the caller provides the source (getrandom in the app,
//! crypto.getRandomValues in the extension). This way the crate depends on no specific source.

use rand_core::{CryptoRng, RngCore};

/// Gerador que preenche bytes com a função dada. Ela DEVE ser uma fonte criptográfica.
/// Generator that fills bytes with the given function. It MUST be a cryptographic source.
pub struct FillRng<F: FnMut(&mut [u8])>(pub F);

impl<F: FnMut(&mut [u8])> RngCore for FillRng<F> {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        (self.0)(&mut b);
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        (self.0)(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        (self.0)(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        (self.0)(dest);
        Ok(())
    }
}

// Marca de "seguro para criptografia": vale porque o contrato exige uma fonte criptográfica.
// "Crypto-safe" marker: valid because the contract requires a cryptographic source.
impl<F: FnMut(&mut [u8])> CryptoRng for FillRng<F> {}
