//! DPAPI: criptografia do Windows presa à conta do usuário. Usada para guardar, fora do cofre,
//! o contador de versão (proteção contra "voltar versão").
//! DPAPI: Windows encryption tied to the user account. Used to store, outside the vault,
//! the version counter (rollback protection).

use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

/// Entropia extra: separa os dados do SigilVault de outros usos da DPAPI na mesma conta.
/// Extra entropy: separates SigilVault data from other DPAPI uses in the same account.
const ENTROPY: &[u8] = b"SigilVault revision guard v1";

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

fn run(data: &[u8], label: &[u8], protect: bool) -> Option<Vec<u8>> {
    let input = blob(data);
    let entropy = blob(label);
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: as entradas apontam para fatias vivas durante a chamada; a saída é alocada pelo
    // Windows, copiada e liberada com LocalFree, como a documentação exige.
    // SAFETY: inputs point to slices alive during the call; the output is allocated by Windows,
    // copied and released with LocalFree, as the documentation requires.
    unsafe {
        let ok = if protect {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok == 0 || output.pbData.is_null() {
            return None;
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as HLOCAL);
        Some(out)
    }
}

/// Protege dados com a conta atual do Windows. / Protects data with the current Windows account.
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    run(data, ENTROPY, true)
}

/// Desfaz [`protect`]. Falha se o arquivo veio de outra conta ou foi alterado.
/// Reverses [`protect`]. Fails if the file came from another account or was altered.
pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    run(data, ENTROPY, false)
}

/// Como [`protect`], com um rótulo próprio: dados de um uso não abrem em outro.
/// Like [`protect`], with its own label: data from one use does not open in another.
pub fn protect_labeled(data: &[u8], label: &[u8]) -> Option<Vec<u8>> {
    run(data, label, true)
}

/// Desfaz [`protect_labeled`] com o mesmo rótulo. / Reverses [`protect_labeled`] with the same label.
pub fn unprotect_labeled(data: &[u8], label: &[u8]) -> Option<Vec<u8>> {
    run(data, label, false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip_and_tamper() {
        let sealed = super::protect(b"{\"abc\":3}").unwrap();
        assert_ne!(sealed, b"{\"abc\":3}");
        assert_eq!(super::unprotect(&sealed).unwrap(), b"{\"abc\":3}");
        let mut bad = sealed.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(super::unprotect(&bad).is_none());
        // Rótulo diferente não abre. / A different label does not open.
        let labeled = super::protect_labeled(b"x", b"a").unwrap();
        assert_eq!(super::unprotect_labeled(&labeled, b"a").unwrap(), b"x");
        assert!(super::unprotect_labeled(&labeled, b"b").is_none());
        assert!(super::unprotect(&labeled).is_none());
    }
}
