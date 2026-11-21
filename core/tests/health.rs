//! Relatório de saúde. / Health report.

use std::time::{SystemTime, UNIX_EPOCH};

use sigilvault_core::{KdfParams, NewItem, Vault};

const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";
const DAY: u64 = 86_400;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn vault() -> Vault {
    Vault::create("ana", PW, KdfParams::MINIMUM).unwrap().0
}

#[test]
fn finds_weak_reused_and_ignores_good_ones() {
    let mut v = vault();
    let weak = v.add_item(NewItem::login("Fraca", "ana", "https://a.example", "senha123")).unwrap();
    let r1 = v.add_item(NewItem::login("Rep 1", "ana", "https://b.example", "Gp7#vQ2!mZr9@Lw4")).unwrap();
    let r2 = v.add_item(NewItem::login("Rep 2", "ana", "https://c.example", "Gp7#vQ2!mZr9@Lw4")).unwrap();
    let good = v.add_item(NewItem::login("Boa", "ana", "https://d.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    // Nota sem senha não entra. / A note without a password is not counted.
    v.add_item(NewItem::note("Nota", "texto")).unwrap();

    let r = v.health_report(now()).unwrap();
    assert_eq!(r.checked, 4);
    assert_eq!(r.entries.len(), 3);
    assert!(!r.entries.iter().any(|e| e.id == good));
    // A fraca vem primeiro. / The weak one comes first.
    assert_eq!(r.entries[0].id, weak);
    assert!(r.entries[0].weak && r.entries[0].reused_with == 0);
    let e1 = r.entries.iter().find(|e| e.id == r1).unwrap();
    let e2 = r.entries.iter().find(|e| e.id == r2).unwrap();
    assert_eq!((e1.reused_with, e2.reused_with), (1, 1));
    assert!(e1.reuse_group.is_some() && e1.reuse_group == e2.reuse_group);
    assert!(!e1.weak && !e1.old);
}

#[test]
fn username_counts_against_strength() {
    let mut v = vault();
    // Senha = usuário + ano: fraca mesmo sendo longa. / Password = username + year: weak even though long.
    let id = v.add_item(NewItem::login("X", "marcelo.figueiredo", "https://x.example", "marcelo.figueiredo2024")).unwrap();
    let r = v.health_report(now()).unwrap();
    assert!(r.entries.iter().any(|e| e.id == id && e.weak));
}

#[test]
fn age_counts_from_the_last_password_change() {
    let mut v = vault();
    let id = v.add_item(NewItem::login("Velha", "ana", "https://a.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    let in_two_years = now() + 2 * 365 * DAY;
    let e = v.health_report(in_two_years).unwrap().entries.into_iter().find(|e| e.id == id).unwrap();
    assert!(e.old && e.age_days >= 729);

    // Trocar a senha zera a idade. / Changing the password resets the age.
    let mut v2 = vault();
    let id2 = v2.add_item(NewItem::login("Nova", "ana", "https://a.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    v2.set_password(id2, "Rb3@nL8!sQ6#yH1w").unwrap();
    let r = v2.health_report(now() + 30 * DAY).unwrap();
    assert!(r.entries.is_empty(), "{:?}", r.entries);
}

#[test]
fn reuse_groups_are_separate() {
    let mut v = vault();
    let a1 = v.add_item(NewItem::login("A1", "u", "https://a.example", "Gp7#vQ2!mZr9@Lw4")).unwrap();
    let a2 = v.add_item(NewItem::login("A2", "u", "https://b.example", "Gp7#vQ2!mZr9@Lw4")).unwrap();
    let b1 = v.add_item(NewItem::login("B1", "u", "https://c.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    let b2 = v.add_item(NewItem::login("B2", "u", "https://d.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    let b3 = v.add_item(NewItem::login("B3", "u", "https://e.example", "xT9$kq4#Wm2!pZ7v")).unwrap();
    let r = v.health_report(now()).unwrap();
    let g = |id| r.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!(g(a1).reuse_group, g(a2).reuse_group);
    assert_eq!(g(b1).reuse_group, g(b3).reuse_group);
    assert_ne!(g(a1).reuse_group, g(b2).reuse_group);
    assert_eq!(g(b2).reused_with, 2);
}
