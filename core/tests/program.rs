//! Fase 6 no núcleo: itens do tipo Programa (Auto-Type).
//! Phase 6 core: Program items (Auto-Type).

use sigilvault_core::{ItemKind, KdfParams, NewItem, Vault, import};

const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";
const EXPORT_PW: &str = "lanterna-musgo-vento-pedra-rio-trigo";
const KDF: KdfParams = KdfParams::MINIMUM;

fn program(title: &str, path: &str, user: &str, pw: &str) -> NewItem {
    let mut i = NewItem::default();
    i.kind = ItemKind::Program;
    i.title = title.into();
    i.username = user.into();
    i.password = Some(pw.into());
    i.app_path = path.into();
    i.app_signer = Some("ab".repeat(32));
    i
}

fn titles(v: &Vault, path: &str) -> Vec<String> {
    let mut t: Vec<String> = v.programs_for(path).iter().map(|i| i.title().to_owned()).collect();
    t.sort();
    t
}

#[test]
fn programs_match_by_executable_path_only() {
    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    v.add_item(program("Banco Desktop", r"C:\Program Files\Banco\banco.exe", "ana", "s1")).unwrap();
    v.add_item(program("Banco 2", r"c:/program files/banco/BANCO.EXE", "bia", "s2")).unwrap();
    v.add_item(program("Sem senha", r"C:\Program Files\Banco\banco.exe", "x", "")).unwrap();
    // Um login de site com o mesmo texto no endereço não conta. / A website login with the same text does not count.
    v.add_item(NewItem::login("Site", "ana", r"C:\Program Files\Banco\banco.exe", "s3")).unwrap();

    assert_eq!(titles(&v, r"C:\PROGRAM FILES\BANCO\banco.exe"), ["Banco 2", "Banco Desktop"]);
    // Mesmo nome de arquivo em outra pasta: não é o mesmo programa. / Same file name elsewhere: not the same program.
    assert!(titles(&v, r"C:\Users\ana\Downloads\banco.exe").is_empty());
    assert!(titles(&v, "").is_empty());
}

#[test]
fn programs_survive_save_backup_export_and_csv() {
    let (mut v, _) = Vault::create("ana", PW, KDF).unwrap();
    let id = v.add_item(program("Banco Desktop", r"C:\Banco\banco.exe", "ana", "s1")).unwrap();
    let check = |v: &Vault| {
        let i = v.items().iter().find(|i| i.title() == "Banco Desktop").unwrap();
        assert_eq!(i.kind(), ItemKind::Program);
        assert_eq!(i.app_path(), r"C:\Banco\banco.exe");
        assert_eq!(v.programs_for(r"c:\banco\banco.exe").len(), 1);
    };
    let mut v = Vault::open(&v.to_bytes().unwrap(), PW).unwrap();
    check(&v);
    assert_eq!(v.item(id).unwrap().app_signer(), Some("ab".repeat(32).as_str()));

    let backup = v.create_backup().unwrap();
    v.remove_item(id).unwrap();
    v.restore_backup(&backup).unwrap();
    check(&v);

    let copy = Vault::open(&v.export_with_password(EXPORT_PW, KDF).unwrap(), EXPORT_PW).unwrap();
    check(&copy);

    // CSV legível: o caminho vai na coluna "url" e volta para o lugar certo.
    // Readable CSV: the path goes in the "url" column and comes back to the right place.
    let csv = v.export_csv().unwrap();
    let parsed = import::parse(&csv).unwrap();
    let (mut fresh, _) = Vault::create("bia", PW, KDF).unwrap();
    fresh.import_items(&parsed.items).unwrap();
    check(&fresh);
}
