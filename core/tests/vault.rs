//! Testes de integração da Fase 1, usando só a API pública.
//! Phase 1 integration tests, using only the public API.
//!
//! Critérios de "pronto": senha errada, código usado duas vezes e arquivo adulterado.
//! "Done" criteria: wrong password, code used twice and tampered file.

use sigilvault_core::{Error, KdfParams, NewItem, Vault};

const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";
const NEW_PW: &str = "farol-pedra-rio-azul-trigo-sino";
/// Parâmetros mínimos para os testes rodarem rápido (ainda 64 MiB).
/// Minimum parameters so tests run fast (still 64 MiB).
const KDF: KdfParams = KdfParams::MINIMUM;

/// Cria um cofre com dois itens e devolve os bytes do arquivo e os códigos em texto.
/// Creates a vault with two items and returns the file bytes and the codes as text.
fn sample_vault() -> (Vec<u8>, Vec<String>) {
    let (mut v, codes) = Vault::create("ana", PW, KDF).unwrap();
    v.add_item(NewItem::login("Banco", "ana@example.com", "https://banco.example", "S3nh@-do-banco")).unwrap();
    v.add_item(NewItem::note("Wi-Fi", "rede: Casa / senha: 12345")).unwrap();
    let codes = codes.iter().map(|c| c.to_display_string().to_string()).collect();
    (v.to_bytes().unwrap(), codes)
}

fn bank_password(v: &Vault) -> String {
    let bank = v.search("banco")[0].id();
    v.reveal_password(bank).unwrap().unwrap().to_string()
}

// ── Criar, abrir e salvar / Create, open and save ─────────────────

#[test]
fn create_save_open_roundtrip() {
    let (bytes, codes) = sample_vault();
    assert_eq!(codes.len(), 10);
    let v = Vault::open(&bytes, PW).unwrap();
    assert_eq!(v.username(), "ana");
    assert_eq!(v.items().len(), 2);
    assert_eq!(v.remaining_recovery_codes(), 10);
    assert_eq!(bank_password(&v), "S3nh@-do-banco");
    let note = v.search("wi-fi")[0].id();
    assert_eq!(&*v.reveal_notes(note).unwrap().unwrap(), "rede: Casa / senha: 12345");
}

#[test]
fn file_contains_no_plaintext() {
    let (bytes, _) = sample_vault();
    let haystack = String::from_utf8_lossy(&bytes);
    for secret in ["ana", "Banco", "banco.example", "S3nh@", "Wi-Fi", "12345"] {
        assert!(!haystack.contains(secret), "vazou / leaked: {secret}");
    }
}

#[test]
fn revision_grows_on_every_save() {
    let (bytes, _) = sample_vault();
    let mut v = Vault::open(&bytes, PW).unwrap();
    let r = v.revision();
    let again = v.to_bytes().unwrap();
    assert_eq!(v.revision(), r + 1);
    assert_eq!(Vault::open(&again, PW).unwrap().revision(), r + 1);
}

#[test]
fn item_crud() {
    let (bytes, _) = sample_vault();
    let mut v = Vault::open(&bytes, PW).unwrap();
    let bank = v.search("BANCO")[0].id();
    v.set_password(bank, "nova-senha").unwrap();
    let note = v.search("wi-fi")[0].id();
    v.remove_item(note).unwrap();
    assert!(matches!(v.remove_item(note), Err(Error::ItemNotFound)));
    assert!(matches!(v.reveal_password(note), Err(Error::ItemNotFound)));
    assert!(matches!(v.add_item(NewItem::login("  ", "", "", "x")), Err(Error::InvalidInput(_))));

    let reopened = Vault::open(&v.to_bytes().unwrap(), PW).unwrap();
    assert_eq!(reopened.items().len(), 1);
    assert_eq!(bank_password(&reopened), "nova-senha");
}

#[test]
fn save_to_disk_is_atomic_and_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("sigilvault-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cofre.sigil");

    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    v.save(&path).unwrap();
    v.add_item(NewItem::login("Site", "ana", "https://site.example", "x")).unwrap();
    v.save(&path).unwrap(); // sobrescreve o existente / overwrites the existing one

    assert!(!dir.join("cofre.sigil.tmp").exists());
    assert_eq!(Vault::open_file(&path, PW).unwrap().items().len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn default_kdf_params_work() {
    let (mut v, _) = Vault::create("ana", PW, KdfParams::DEFAULT).unwrap();
    let bytes = v.to_bytes().unwrap();
    assert_eq!(Vault::open(&bytes, PW).unwrap().kdf_params(), KdfParams::DEFAULT);
}

#[test]
fn unicode_password_is_normalized() {
    // "é" composto ao criar, decomposto ao abrir (teclados diferentes).
    // Composed "é" on create, decomposed on open (different keyboards).
    let composed = "caf\u{e9}-trilho-nuvem-sete-farol-mesa";
    let decomposed = "cafe\u{301}-trilho-nuvem-sete-farol-mesa";
    let (mut v, _) = Vault::create("ana", composed, KDF).unwrap();
    assert!(Vault::open(&v.to_bytes().unwrap(), decomposed).is_ok());
}

// ── Senha errada e senha fraca / Wrong and weak password ──────────

#[test]
fn wrong_password_is_rejected() {
    let (bytes, _) = sample_vault();
    for wrong in ["", "cavalo-trilho-nuvem-sete-farol-mes", "CAVALO-TRILHO-NUVEM-SETE-FAROL-MESA", NEW_PW] {
        assert!(matches!(Vault::open(&bytes, wrong), Err(Error::InvalidCredentials)), "{wrong}");
    }
}

#[test]
fn weak_master_password_is_rejected() {
    assert!(matches!(Vault::create("ana", "senha123", KDF), Err(Error::WeakMasterPassword)));
    assert!(matches!(Vault::create("   ", PW, KDF), Err(Error::InvalidInput(_))));
    let weak_kdf = KdfParams { memory_kib: 1024, ..KDF };
    assert!(matches!(Vault::create("ana", PW, weak_kdf), Err(Error::WeakKdfParams)));
}

#[test]
fn change_master_password_keeps_codes_valid() {
    let (bytes, codes) = sample_vault();
    let mut v = Vault::open(&bytes, PW).unwrap();
    assert!(matches!(v.change_master_password("errada", NEW_PW), Err(Error::InvalidCredentials)));
    assert!(matches!(v.change_master_password(PW, "123456"), Err(Error::WeakMasterPassword)));
    v.change_master_password(PW, NEW_PW).unwrap();
    let bytes = v.to_bytes().unwrap();

    assert!(matches!(Vault::open(&bytes, PW), Err(Error::InvalidCredentials)));
    let v = Vault::open(&bytes, NEW_PW).unwrap();
    assert_eq!(bank_password(&v), "S3nh@-do-banco");
    // Os códigos continuam valendo. / Codes remain valid.
    assert!(Vault::recover(&bytes, &codes[0], PW).is_ok());
}

// ── Códigos de recuperação / Recovery codes ───────────────────────

#[test]
fn recover_with_code_shows_username_and_counts_down() {
    let (bytes, codes) = sample_vault();
    let (mut v, outcome) = Vault::recover(&bytes, &codes[3], NEW_PW).unwrap();
    assert_eq!(outcome.username, "ana");
    assert_eq!(outcome.remaining_codes, 9);
    assert_eq!(bank_password(&v), "S3nh@-do-banco");

    let bytes = v.to_bytes().unwrap();
    assert!(matches!(Vault::open(&bytes, PW), Err(Error::InvalidCredentials)));
    assert!(Vault::open(&bytes, NEW_PW).is_ok());
}

#[test]
fn code_used_twice_is_rejected() {
    let (bytes, codes) = sample_vault();
    let (mut v, _) = Vault::recover(&bytes, &codes[0], NEW_PW).unwrap();
    let after_first_use = v.to_bytes().unwrap();

    let second = Vault::recover(&after_first_use, &codes[0], PW);
    assert!(matches!(second, Err(Error::InvalidCredentials)));

    // Os outros 9 continuam valendo, e a contagem segue caindo.
    // The other 9 stay valid, and the count keeps going down.
    let (mut v, outcome) = Vault::recover(&after_first_use, &codes[1], PW).unwrap();
    assert_eq!(outcome.remaining_codes, 8);
    let after_second_use = v.to_bytes().unwrap();
    assert!(Vault::recover(&after_second_use, &codes[1], NEW_PW).is_err());
}

#[test]
fn all_ten_codes_can_be_used_once() {
    let (mut bytes, codes) = sample_vault();
    for (i, code) in codes.iter().enumerate() {
        let (mut v, outcome) = Vault::recover(&bytes, code, PW).unwrap();
        assert_eq!(outcome.remaining_codes, 9 - i);
        bytes = v.to_bytes().unwrap();
    }
    for code in &codes {
        assert!(Vault::recover(&bytes, code, PW).is_err());
    }
    assert_eq!(Vault::open(&bytes, PW).unwrap().items().len(), 2);
}

#[test]
fn codes_are_bound_to_their_vault() {
    let (bytes_a, _) = sample_vault();
    let (_, codes_b) = sample_vault();
    assert!(matches!(Vault::recover(&bytes_a, &codes_b[0], NEW_PW), Err(Error::InvalidCredentials)));
}

#[test]
fn malformed_code_and_weak_new_password() {
    let (bytes, codes) = sample_vault();
    assert!(matches!(Vault::recover(&bytes, "AAAA-BBBB", NEW_PW), Err(Error::MalformedRecoveryCode)));
    // Troca o último caractere (dígito verificador). / Changes the last character (check digit).
    let mut typo = codes[0].clone();
    let last = typo.pop().unwrap();
    typo.push(if last == 'A' { 'B' } else { 'A' });
    assert!(matches!(Vault::recover(&bytes, &typo, NEW_PW), Err(Error::MalformedRecoveryCode)));
    // Senha nova fraca não queima o código. / A weak new password does not burn the code.
    assert!(matches!(Vault::recover(&bytes, &codes[0], "123456"), Err(Error::WeakMasterPassword)));
    assert!(Vault::recover(&bytes, &codes[0], NEW_PW).is_ok());
}

#[test]
fn regenerate_codes_invalidates_old_ones() {
    let (bytes, old_codes) = sample_vault();
    let mut v = Vault::open(&bytes, PW).unwrap();
    assert!(matches!(v.regenerate_recovery_codes("errada"), Err(Error::InvalidCredentials)));
    let new_codes = v.regenerate_recovery_codes(PW).unwrap();
    assert_eq!(new_codes.len(), 10);
    let bytes = v.to_bytes().unwrap();

    for code in &old_codes {
        assert!(matches!(Vault::recover(&bytes, code, NEW_PW), Err(Error::InvalidCredentials)));
    }
    let (_, outcome) = Vault::recover(&bytes, &new_codes[0].to_display_string(), NEW_PW).unwrap();
    assert_eq!(outcome.remaining_codes, 9);
    assert!(Vault::open(&bytes, PW).is_ok());
}

// ── Arquivo adulterado / Tampered file ────────────────────────────

/// Posição onde começa o corpo cifrado. / Position where the encrypted body starts.
fn body_start(bytes: &[u8]) -> usize {
    14 + u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize
}

#[test]
fn tampered_body_is_detected() {
    let (bytes, _) = sample_vault();
    let start = body_start(&bytes);
    let len = bytes.len() - start;
    // Início, meio, fim do corpo e a etiqueta de autenticação.
    // Start, middle and end of the body, and the authentication tag.
    for pos in [start, start + 1, start + len / 2, bytes.len() - 17, bytes.len() - 1] {
        let mut bad = bytes.clone();
        bad[pos] ^= 0x01;
        assert!(matches!(Vault::open(&bad, PW), Err(Error::Tampered)), "byte {pos}");
    }
}

#[test]
fn tampered_header_is_detected() {
    let (bytes, _) = sample_vault();
    let start = body_start(&bytes);
    // Vários pontos do prefixo e do cabeçalho: nenhum pode abrir.
    // Several points of the prefix and header: none may open.
    let step = (start / 25).max(1);
    for pos in (0..start).step_by(step) {
        let mut bad = bytes.clone();
        bad[pos] ^= 0x01;
        assert!(Vault::open(&bad, PW).is_err(), "byte {pos} aceito / accepted");
    }
}

#[test]
fn edited_revision_is_detected() {
    // Mudar só a revisão (para enganar a proteção de versão) quebra a autenticação.
    // Changing only the revision (to fool rollback protection) breaks authentication.
    let (bytes, _) = sample_vault();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert!(text.contains("\"revision\":1,"));
    let header_end = body_start(&bytes);
    let header = String::from_utf8(bytes[14..header_end].to_vec()).unwrap();
    let edited = header.replace("\"revision\":1,", "\"revision\":9,");
    let mut bad = bytes[..14].to_vec();
    bad.extend_from_slice(edited.as_bytes());
    bad.extend_from_slice(&bytes[header_end..]);
    assert!(matches!(Vault::open(&bad, PW), Err(Error::Tampered)));
}

#[test]
fn truncated_or_garbage_files_are_rejected() {
    let (bytes, _) = sample_vault();
    for len in [0, 5, 13, 14, 100, body_start(&bytes), bytes.len() - 1] {
        assert!(Vault::open(&bytes[..len], PW).is_err(), "len {len}");
    }
    assert!(matches!(Vault::open(b"isto nao e um cofre", PW), Err(Error::InvalidFormat(_))));
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(matches!(Vault::open(&extra, PW), Err(Error::Tampered)));
}

#[test]
fn header_from_another_vault_is_rejected() {
    // Colar o cabeçalho (com o slot da senha) de outro cofre com a mesma senha não abre o corpo.
    // Pasting the header (with the password slot) of another vault with the same password does not open the body.
    let (a, _) = sample_vault();
    let (b, _) = sample_vault();
    let mut mixed = b[..body_start(&b)].to_vec();
    mixed.extend_from_slice(&a[body_start(&a)..]);
    assert!(matches!(Vault::open(&mixed, PW), Err(Error::Tampered)));
}
