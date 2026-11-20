//! Testes da Fase 3: exportação, backups, importação e mesclagem.
//! Phase 3 tests: export, backups, import and merge.

use sigilvault_core::{Error, ItemKind, KdfParams, NewItem, Vault, import};

const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";
const EXPORT_PW: &str = "lanterna-musgo-vento-pedra-rio-trigo";
const NEW_PW: &str = "farol-pedra-rio-azul-trigo-sino";
const KDF: KdfParams = KdfParams::MINIMUM;

fn sample() -> (Vault, Vec<String>) {
    let (mut v, codes) = Vault::create("ana", PW, KDF).unwrap();
    v.add_item(NewItem::login("Banco", "ana", "https://banco.example", "S3nh@")).unwrap();
    v.add_item(NewItem::note("Wi-Fi", "rede Casa")).unwrap();
    (v, codes.iter().map(|c| c.to_display_string().to_string()).collect())
}

fn password_of(v: &Vault, title: &str) -> Option<String> {
    let id = v.items().iter().find(|i| i.title() == title)?.id();
    v.reveal_password(id).unwrap().map(|p| p.to_string())
}

// ── Exportação / Export ───────────────────────────────────────────

#[test]
fn export_with_own_password_has_no_recovery_codes() {
    let (v, codes) = sample();
    let exported = v.export_with_password(EXPORT_PW, KDF).unwrap();

    let copy = Vault::open(&exported, EXPORT_PW).unwrap();
    assert_eq!(copy.remaining_recovery_codes(), 0);
    assert_eq!(copy.username(), "ana");
    assert_eq!(copy.items().len(), 2);
    assert_eq!(password_of(&copy, "Banco").as_deref(), Some("S3nh@"));
    assert_ne!(copy.vault_id(), v.vault_id(), "cofre novo / new vault");

    // Nem a senha mestra nem os códigos abrem a cópia. / Neither the master password nor the codes open the copy.
    assert!(matches!(Vault::open(&exported, PW), Err(Error::InvalidCredentials)));
    for code in &codes {
        assert!(matches!(Vault::recover(&exported, code, NEW_PW), Err(Error::InvalidCredentials)));
    }
}

#[test]
fn export_password_must_be_strong() {
    let (v, _) = sample();
    assert!(matches!(v.export_with_password("123456", KDF), Err(Error::WeakMasterPassword)));
}

// ── Backups ───────────────────────────────────────────────────────

#[test]
fn backup_inspect_and_restore() {
    let (mut v, _) = sample();
    let backup = v.create_backup().unwrap();
    let info = v.inspect_backup(&backup).unwrap();
    assert_eq!(info.item_count, 2);

    let bank = v.items().iter().find(|i| i.title() == "Banco").unwrap().id();
    v.remove_item(bank).unwrap();
    v.add_item(NewItem::login("Outro", "", "", "x")).unwrap();
    v.restore_backup(&backup).unwrap();
    let titles: Vec<&str> = v.items().iter().map(|i| i.title()).collect();
    assert_eq!(titles.len(), 2);
    assert!(titles.contains(&"Banco") && titles.contains(&"Wi-Fi"));
    assert_eq!(password_of(&v, "Banco").as_deref(), Some("S3nh@"));

    // Backup sobrevive a salvar e reabrir o cofre. / Backup survives saving and reopening the vault.
    let reopened = Vault::open(&v.to_bytes().unwrap(), PW).unwrap();
    assert_eq!(reopened.inspect_backup(&backup).unwrap().item_count, 2);
}

#[test]
fn backup_file_has_no_plaintext_and_detects_tampering() {
    let (v, _) = sample();
    let backup = v.create_backup().unwrap();
    let text = String::from_utf8_lossy(&backup);
    for secret in ["ana", "Banco", "S3nh@", "Wi-Fi"] {
        assert!(!text.contains(secret), "{secret}");
    }
    let mut bad = backup.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(matches!(v.inspect_backup(&bad), Err(Error::Tampered)));
}

#[test]
fn backup_of_another_vault_is_rejected() {
    let (a, _) = sample();
    let (b, _) = sample();
    assert!(b.inspect_backup(&a.create_backup().unwrap()).is_err());
}

#[test]
fn backups_cannot_be_opened_with_old_password_or_used_code() {
    // Backups não têm slots: nem senha nem código abrem o arquivo diretamente.
    // Backups have no slots: neither password nor code opens the file directly.
    let (v, codes) = sample();
    let backup = v.create_backup().unwrap();
    assert!(Vault::open(&backup, PW).is_err());
    assert!(Vault::recover(&backup, &codes[0], NEW_PW).is_err());
}

#[test]
fn rekey_after_password_change_reencrypts_backups() {
    let (mut v, _) = sample();
    let b1 = v.create_backup().unwrap();
    let b2 = v.create_backup().unwrap();
    let old_vault_file = v.to_bytes().unwrap();

    v.change_master_password(PW, NEW_PW).unwrap();
    let rekeyed = v.rekey_backups(&[b1.clone(), b2.clone(), b"lixo".to_vec()]).unwrap();
    assert!(rekeyed[0].is_some() && rekeyed[1].is_some());
    assert!(rekeyed[2].is_none(), "backup ilegível vira None / unreadable backup becomes None");
    let new_b1 = rekeyed[0].clone().unwrap();
    assert_eq!(v.inspect_backup(&new_b1).unwrap().item_count, 2);

    // Os backups antigos não abrem mais com o cofre atual. / Old backups no longer open with the current vault.
    assert!(v.inspect_backup(&b1).is_err());
    // Quem tem o arquivo antigo e a senha antiga não abre os backups novos.
    // Someone with the old file and old password cannot open the new backups.
    let old = Vault::open(&old_vault_file, PW).unwrap();
    assert!(old.inspect_backup(&new_b1).is_err());
    // A chave nova vai junto ao salvar. / The new key is kept on save.
    let reopened = Vault::open(&v.to_bytes().unwrap(), NEW_PW).unwrap();
    assert!(reopened.inspect_backup(&new_b1).is_ok());
}

#[test]
fn rekey_after_recovery_code() {
    let (mut v, codes) = sample();
    let b = v.create_backup().unwrap();
    let file = v.to_bytes().unwrap();
    let (mut recovered, _) = Vault::recover(&file, &codes[0], NEW_PW).unwrap();
    // O backup ainda abre (mesma chave de backup) até o rekey. / The backup still opens (same backup key) until rekey.
    assert!(recovered.inspect_backup(&b).is_ok());
    let out = recovered.rekey_backups(std::slice::from_ref(&b)).unwrap();
    assert!(recovered.inspect_backup(&b).is_err());
    assert!(recovered.inspect_backup(out[0].as_ref().unwrap()).is_ok());
}

// ── Importação e mesclagem / Import and merge ─────────────────────

#[test]
fn import_skips_duplicates() {
    let (mut v, _) = sample();
    let csv = "name,url,username,password\nBanco,https://banco.example,ana,S3nh@\nNovo,https://novo.example,bia,pw\nNovo,https://novo.example,bia,pw\n";
    let parsed = import::parse(csv.as_bytes()).unwrap();
    let report = v.import_items(&parsed.items).unwrap();
    assert_eq!(report.added, 1);
    assert_eq!(report.duplicates, 2);
    assert_eq!(v.items().len(), 3);
    assert_eq!(password_of(&v, "Novo").as_deref(), Some("pw"));
}

#[test]
fn csv_export_import_roundtrip() {
    let (v, _) = sample();
    let csv = v.export_csv().unwrap();
    let parsed = import::parse(&csv).unwrap();
    assert_eq!(parsed.format, import::Format::SigilVaultCsv);
    let (mut other, _) = Vault::create("bia", PW, KDF).unwrap();
    assert_eq!(other.import_items(&parsed.items).unwrap().added, 2);
    assert_eq!(password_of(&other, "Banco").as_deref(), Some("S3nh@"));
    let note = other.items().iter().find(|i| i.title() == "Wi-Fi").unwrap();
    assert_eq!(note.kind(), ItemKind::SecureNote);
    assert_eq!(&*other.reveal_notes(note.id()).unwrap().unwrap(), "rede Casa");
}

#[test]
fn merge_from_another_vault() {
    let (mut a, _) = sample();
    let (mut b, _) = Vault::create("bia", PW, KDF).unwrap();
    b.add_item(NewItem::login("Banco", "ana", "https://banco.example", "S3nh@")).unwrap(); // duplicata / duplicate
    b.add_item(NewItem::login("Loja", "bia", "https://loja.example", "L0ja")).unwrap();
    let report = a.merge_from(&b).unwrap();
    assert_eq!((report.added, report.duplicates), (1, 1));
    assert_eq!(password_of(&a, "Loja").as_deref(), Some("L0ja"));
}

#[test]
fn check_file_accepts_vaults_only() {
    let (mut v, _) = sample();
    assert!(Vault::check_file(&v.to_bytes().unwrap()).is_ok());
    assert!(Vault::check_file(&v.create_backup().unwrap()).is_err());
    assert!(Vault::check_file(b"name,url\n").is_err());
}
