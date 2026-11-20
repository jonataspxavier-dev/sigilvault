//! Testes da Fase 5 no núcleo: quais logins servem para uma página, e o histórico de senhas.
//! Phase 5 core tests: which logins fit a page, and the password history.

use sigilvault_core::{ItemUpdate, KdfParams, MAX_HISTORY, NewItem, Origin, Vault};

const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";
const NEW_PW: &str = "farol-pedra-rio-azul-trigo-sino";
const EXPORT_PW: &str = "lanterna-musgo-vento-pedra-rio-trigo";
const KDF: KdfParams = KdfParams::MINIMUM;

fn page(s: &str) -> Origin {
    Origin::parse(s).unwrap()
}

fn titles(v: &Vault, url: &str) -> Vec<String> {
    let mut t: Vec<String> = v.logins_for(&page(url)).iter().map(|i| i.title().to_owned()).collect();
    t.sort();
    t
}

fn history(v: &Vault, title: &str) -> Vec<String> {
    let id = v.items().iter().find(|i| i.title() == title).unwrap().id();
    v.reveal_history(id).unwrap().into_iter().map(|(_, p)| p.to_string()).collect()
}

#[test]
fn logins_for_page_follow_the_origin_rule() {
    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    v.add_item(NewItem::login("Banco", "ana", "https://banco.example", "a")).unwrap();
    v.add_item(NewItem::login("Banco 2", "bia", "banco.example/entrar", "b")).unwrap();
    v.add_item(NewItem::login("Antigo", "ana", "http://banco.example", "c")).unwrap();
    v.add_item(NewItem::login("Sem senha", "ana", "https://banco.example", "")).unwrap();
    v.add_item(NewItem::note("Nota", "https://banco.example")).unwrap();
    let mut sub = NewItem::login("Loja", "ana", "https://loja.example", "d");
    sub.match_subdomains = true;
    v.add_item(sub).unwrap();

    // Várias contas no mesmo site; http e https separados; sem senha e notas nunca.
    // Several accounts on the same site; http and https apart; no password and notes never.
    assert_eq!(titles(&v, "https://banco.example/login"), ["Banco", "Banco 2"]);
    assert_eq!(titles(&v, "http://banco.example"), ["Antigo"]);
    assert!(titles(&v, "https://banco.example.golpe.net").is_empty());
    assert!(titles(&v, "https://login.banco.example").is_empty());
    assert_eq!(titles(&v, "https://conta.loja.example"), ["Loja"]);
}

#[test]
fn password_changes_go_to_history() {
    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    let id = v.add_item(NewItem::login("Banco", "ana", "https://banco.example", "v0")).unwrap();
    // Mesma senha não gera histórico. / The same password does not create history.
    v.set_password(id, "v0").unwrap();
    assert!(history(&v, "Banco").is_empty());

    v.set_password(id, "v1").unwrap();
    let mut up = ItemUpdate::default();
    up.title = "Banco".into();
    up.url = "https://banco.example".into();
    up.password = Some("v2".into());
    v.update_item(id, up).unwrap();
    // Editar sem mexer na senha não gera histórico. / Editing without touching the password adds nothing.
    let mut up = ItemUpdate::default();
    up.title = "Banco".into();
    up.url = "https://banco.example".into();
    up.match_subdomains = true;
    v.update_item(id, up).unwrap();
    assert!(v.item(id).unwrap().match_subdomains());
    assert_eq!(history(&v, "Banco"), ["v1", "v0"]);

    // Só as últimas MAX_HISTORY ficam. / Only the last MAX_HISTORY stay.
    for n in 3..30 {
        v.set_password(id, &format!("v{n}")).unwrap();
    }
    let h = history(&v, "Banco");
    assert_eq!(h.len(), MAX_HISTORY);
    assert_eq!(h[0], "v28");
    assert_eq!(v.reveal_password(id).unwrap().unwrap().as_str(), "v29");
}

#[test]
fn history_survives_save_rotation_backup_and_export() {
    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    let mut item = NewItem::login("Banco", "ana", "https://banco.example", "v0");
    item.match_subdomains = true;
    let id = v.add_item(item).unwrap();
    v.set_password(id, "v1").unwrap();

    // Gravar e abrir. / Save and open.
    let mut v = Vault::open(&v.to_bytes().unwrap(), PW).unwrap();
    assert_eq!(history(&v, "Banco"), ["v0"]);

    // Troca da chave do cofre (troca da senha mestra) recriptografa o histórico.
    // Vault key rotation (master password change) re-encrypts the history.
    v.change_master_password(PW, NEW_PW).unwrap();
    let mut v = Vault::open(&v.to_bytes().unwrap(), NEW_PW).unwrap();
    assert_eq!(history(&v, "Banco"), ["v0"]);

    // Backup e restauração. / Backup and restore.
    let backup = v.create_backup().unwrap();
    let id = v.items()[0].id();
    v.set_password(id, "v2").unwrap();
    v.restore_backup(&backup).unwrap();
    assert_eq!(history(&v, "Banco"), ["v0"]);
    assert!(v.items()[0].match_subdomains());

    // Cópia com senha própria. / Copy with its own password.
    let copy = Vault::open(&v.export_with_password(EXPORT_PW, KDF).unwrap(), EXPORT_PW).unwrap();
    assert_eq!(history(&copy, "Banco"), ["v0"]);
    assert!(copy.items()[0].match_subdomains());
}
