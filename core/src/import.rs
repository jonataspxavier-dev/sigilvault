//! Leitura de exportações de outros gerenciadores: Chrome, Firefox, Bitwarden (CSV e JSON),
//! KeePass 2 (XML), KeePassXC (CSV) e o CSV do próprio SigilVault.
//! Reading exports from other managers: Chrome, Firefox, Bitwarden (CSV and JSON),
//! KeePass 2 (XML), KeePassXC (CSV) and SigilVault's own CSV.
//!
//! Tudo aqui trata a entrada como hostil: limites de tamanho, nada de `panic`, e o XML é lido
//! sem DTD nem entidades externas (sem ataque XXE).
//! Everything here treats input as hostile: size limits, no `panic`, and XML is read without
//! DTD or external entities (no XXE attack).

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::vault::{ItemKind, NewItem};

/// Maior arquivo aceito. / Largest accepted file.
pub const MAX_IMPORT_LEN: usize = 64 * 1024 * 1024;
/// Máximo de itens por importação. / Maximum items per import.
pub const MAX_ITEMS: usize = 100_000;
/// Maior campo aceito; acima disso a linha é descartada. / Largest accepted field; above it the row is dropped.
const MAX_FIELD: usize = 64 * 1024;
/// Profundidade máxima do XML. / Maximum XML depth.
const MAX_XML_DEPTH: usize = 256;

/// Cabeçalho do CSV do SigilVault. / SigilVault CSV header.
pub const SIGILVAULT_CSV_HEADER: [&str; 7] = ["kind", "title", "username", "url", "password", "notes", "tags"];

/// Formato reconhecido. / Recognized format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// CSV do próprio SigilVault. / SigilVault's own CSV.
    SigilVaultCsv,
    /// Google Chrome (e Edge, Brave, Opera). / Google Chrome (and Edge, Brave, Opera).
    ChromeCsv,
    /// Mozilla Firefox.
    FirefoxCsv,
    /// Bitwarden em CSV. / Bitwarden as CSV.
    BitwardenCsv,
    /// Bitwarden em JSON (não cifrado). / Bitwarden as JSON (unencrypted).
    BitwardenJson,
    /// KeePass 2 XML.
    KeePassXml,
    /// KeePassXC CSV.
    KeePassXcCsv,
    /// CSV genérico com colunas reconhecíveis. / Generic CSV with recognizable columns.
    GenericCsv,
}

/// Resultado da leitura. Sem `Debug` de propósito: contém senhas.
/// Read result. No `Debug` on purpose: it holds passwords.
pub struct Parsed {
    /// Formato detectado. / Detected format.
    pub format: Format,
    /// Itens válidos. / Valid items.
    pub items: Vec<NewItem>,
    /// Linhas ignoradas (malformadas, vazias ou de tipo não suportado).
    /// Skipped rows (malformed, empty or of an unsupported type).
    pub skipped: usize,
}

/// Campos intermediários de uma entrada. / Intermediate fields of an entry.
#[derive(Default)]
struct Raw {
    kind: Option<ItemKind>,
    title: String,
    username: String,
    url: String,
    password: Zeroizing<String>,
    notes: Zeroizing<String>,
    tags: Vec<String>,
}

/// Pega o host de uma URL simples (sem biblioteca: só para sugerir um título).
/// Gets the host of a simple URL (no library: only to suggest a title).
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    host.split(':').next().unwrap_or("")
}

impl Raw {
    fn too_big(&self) -> bool {
        [&self.title, &self.username, &self.url, &*self.password, &*self.notes].iter().any(|f| f.len() > MAX_FIELD)
            || self.tags.iter().map(String::len).sum::<usize>() > MAX_FIELD
    }

    /// Vira um item, escolhendo um título se faltar. `None` se não houver nada útil.
    /// Becomes an item, picking a title if missing. `None` if there is nothing useful.
    fn into_item(self) -> Option<NewItem> {
        if self.too_big() {
            return None;
        }
        let kind = self.kind.unwrap_or(
            if self.password.is_empty() && self.username.is_empty() && self.url.is_empty() && !self.notes.is_empty() {
                ItemKind::SecureNote
            } else {
                ItemKind::Login
            },
        );
        if self.password.is_empty() && self.username.is_empty() && self.url.is_empty() && self.notes.is_empty() {
            return None;
        }
        let title = [self.title.trim(), host_of(self.url.trim()), self.username.trim()]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or("(sem título / untitled)")
            .chars()
            .take(200)
            .collect();
        let mut item = NewItem::default();
        item.kind = kind;
        item.title = title;
        item.username = self.username.trim().to_owned();
        // Programa: no CSV do SigilVault, a coluna "url" traz o caminho do executável.
        // Program: in the SigilVault CSV, the "url" column carries the executable path.
        if kind == ItemKind::Program {
            item.app_path = self.url.trim().to_owned();
        } else {
            item.url = self.url.trim().to_owned();
        }
        item.tags = self.tags.iter().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
        item.password = (!self.password.is_empty()).then(|| self.password.to_string());
        item.notes = (!self.notes.is_empty()).then(|| self.notes.to_string());
        Some(item)
    }
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Lê o arquivo e detecta o formato. / Reads the file and detects the format.
pub fn parse(bytes: &[u8]) -> Result<Parsed> {
    if bytes.len() > MAX_IMPORT_LEN {
        return Err(Error::UnsupportedImport("arquivo grande demais / file too large"));
    }
    let text = Zeroizing::new(String::from_utf8_lossy(bytes).into_owned());
    let text = strip_bom(&text);
    match text.trim_start().chars().next() {
        Some('<') => parse_keepass_xml(text),
        Some('{') => parse_bitwarden_json(text),
        Some(_) => parse_csv(text),
        None => Err(Error::UnsupportedImport("arquivo vazio / empty file")),
    }
}

// ── CSV ───────────────────────────────────────────────────────────

fn detect_csv(cols: &[String]) -> Option<Format> {
    let has = |c: &str| cols.iter().any(|x| x == c);
    if SIGILVAULT_CSV_HEADER.iter().all(|c| has(c)) {
        Some(Format::SigilVaultCsv)
    } else if has("login_password") && has("name") {
        Some(Format::BitwardenCsv)
    } else if has("group") && has("title") && has("password") {
        Some(Format::KeePassXcCsv)
    } else if has("httprealm") && has("password") && has("url") {
        Some(Format::FirefoxCsv)
    } else if has("name") && has("url") && has("username") && has("password") {
        Some(Format::ChromeCsv)
    } else if has("password") && (has("url") || has("title") || has("name")) {
        Some(Format::GenericCsv)
    } else {
        None
    }
}

fn parse_csv(text: &str) -> Result<Parsed> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).has_headers(true).from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|_| Error::UnsupportedImport("CSV ilegível / unreadable CSV"))?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let format = detect_csv(&headers).ok_or(Error::UnsupportedImport("colunas não reconhecidas / unrecognized columns"))?;
    let index: HashMap<&str, usize> = headers.iter().enumerate().map(|(i, h)| (h.as_str(), i)).collect();

    let mut items = Vec::new();
    let mut skipped = 0;
    for record in reader.records() {
        if items.len() >= MAX_ITEMS {
            skipped += 1;
            continue;
        }
        let Ok(record) = record else {
            skipped += 1;
            continue;
        };
        let get = |name: &str| index.get(name).and_then(|&i| record.get(i)).unwrap_or("").to_owned();
        let first = |names: &[&str]| names.iter().map(|n| get(n)).find(|v| !v.is_empty()).unwrap_or_default();
        let mut raw = Raw::default();
        match format {
            Format::SigilVaultCsv => {
                raw.kind = Some(match get("kind").as_str() {
                    "secure_note" => ItemKind::SecureNote,
                    "program" => ItemKind::Program,
                    _ => ItemKind::Login,
                });
                raw.title = get("title");
                raw.username = get("username");
                raw.url = get("url");
                *raw.password = get("password");
                *raw.notes = get("notes");
                raw.tags = get("tags").split(',').map(str::to_owned).collect();
            }
            Format::BitwardenCsv => {
                raw.kind = match get("type").as_str() {
                    "login" | "" => Some(ItemKind::Login),
                    "note" => Some(ItemKind::SecureNote),
                    _ => {
                        // Cartões e identidades ainda não são suportados. / Cards and identities are not supported yet.
                        skipped += 1;
                        continue;
                    }
                };
                raw.title = get("name");
                raw.username = get("login_username");
                raw.url = get("login_uri").split(',').next().unwrap_or("").to_owned();
                *raw.password = get("login_password");
                let fields = get("fields");
                *raw.notes = [get("notes"), fields].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n");
                raw.tags = vec![get("folder")];
            }
            Format::KeePassXcCsv => {
                raw.title = get("title");
                raw.username = get("username");
                raw.url = get("url");
                *raw.password = get("password");
                *raw.notes = get("notes");
                let group = get("group");
                let group = group.strip_prefix("Root/").or_else(|| group.strip_prefix("Root")).unwrap_or(&group);
                raw.tags = vec![group.trim_matches('/').to_owned()];
            }
            Format::FirefoxCsv => {
                raw.url = get("url");
                raw.username = get("username");
                *raw.password = get("password");
            }
            Format::ChromeCsv | Format::GenericCsv => {
                raw.title = first(&["name", "title"]);
                raw.url = first(&["url", "uri", "website", "login_uri"]);
                raw.username = first(&["username", "login", "user", "email", "login_username"]);
                *raw.password = first(&["password", "login_password"]);
                *raw.notes = first(&["note", "notes", "comment", "extra"]);
            }
            Format::BitwardenJson | Format::KeePassXml => unreachable!("não é CSV / not CSV"),
        }
        match raw.into_item() {
            Some(item) => items.push(item),
            None => skipped += 1,
        }
    }
    Ok(Parsed { format, items, skipped })
}

// ── Bitwarden JSON ────────────────────────────────────────────────

fn parse_bitwarden_json(text: &str) -> Result<Parsed> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| Error::UnsupportedImport("JSON ilegível / unreadable JSON"))?;
    if value.get("encrypted").and_then(|v| v.as_bool()) == Some(true) {
        return Err(Error::UnsupportedImport(
            "exportação cifrada do Bitwarden; exporte sem senha / encrypted Bitwarden export; export without password",
        ));
    }
    let list = value.get("items").and_then(|v| v.as_array()).ok_or(Error::UnsupportedImport("JSON sem itens / JSON without items"))?;
    let str_of = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_owned();
    let folders: HashMap<String, String> = value
        .get("folders")
        .and_then(|v| v.as_array())
        .map(|f| f.iter().map(|x| (str_of(x, "id"), str_of(x, "name"))).collect())
        .unwrap_or_default();

    let mut items = Vec::new();
    let mut skipped = 0;
    for entry in list {
        if items.len() >= MAX_ITEMS {
            skipped += 1;
            continue;
        }
        let kind = match entry.get("type").and_then(|t| t.as_u64()) {
            Some(1) => ItemKind::Login,
            Some(2) => ItemKind::SecureNote,
            _ => {
                skipped += 1;
                continue;
            }
        };
        let mut raw = Raw { kind: Some(kind), title: str_of(entry, "name"), ..Default::default() };
        let mut notes = vec![str_of(entry, "notes")];
        if let Some(fields) = entry.get("fields").and_then(|f| f.as_array()) {
            for f in fields {
                notes.push(format!("{}: {}", str_of(f, "name"), str_of(f, "value")));
            }
        }
        *raw.notes = notes.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
        if let Some(login) = entry.get("login") {
            raw.username = str_of(login, "username");
            *raw.password = str_of(login, "password");
            raw.url = login
                .get("uris")
                .and_then(|u| u.as_array())
                .and_then(|u| u.first())
                .map(|u| str_of(u, "uri"))
                .unwrap_or_default();
        }
        if let Some(folder) = entry.get("folderId").and_then(|f| f.as_str()).and_then(|id| folders.get(id)) {
            raw.tags = vec![folder.clone()];
        }
        match raw.into_item() {
            Some(item) => items.push(item),
            None => skipped += 1,
        }
    }
    Ok(Parsed { format: Format::BitwardenJson, items, skipped })
}

// ── KeePass 2 XML ─────────────────────────────────────────────────

/// Resolve `&amp;`, `&#233;` etc. / Resolves `&amp;`, `&#233;` etc.
fn resolve_reference(r: &quick_xml::events::BytesRef<'_>) -> Option<String> {
    if r.is_char_ref() {
        return r.resolve_char_ref().ok().flatten().map(String::from);
    }
    Some(
        match &*r.clone().into_inner() {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            // Entidades desconhecidas não são expandidas (nada de DTD). / Unknown entities are not expanded (no DTD).
            _ => return None,
        }
        .to_owned(),
    )
}

fn parse_keepass_xml(text: &str) -> Result<Parsed> {
    let bad = || Error::UnsupportedImport("XML ilegível / unreadable XML");
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);

    let mut path: Vec<String> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    let mut history_depth = 0usize;
    let mut entry: Option<HashMap<String, Zeroizing<String>>> = None;
    let mut key = String::new();
    let mut value: Option<Zeroizing<String>> = None;
    let mut buf = Zeroizing::new(String::new());
    let mut saw_keepass = false;

    let mut items = Vec::new();
    let mut skipped = 0;

    loop {
        match reader.read_event().map_err(|_| bad())? {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_owned();
                if path.len() >= MAX_XML_DEPTH {
                    return Err(bad());
                }
                match name.as_str() {
                    "KeePassFile" => saw_keepass = true,
                    "Group" => groups.push(String::new()),
                    "History" => history_depth += 1,
                    "Entry" if history_depth == 0 => entry = Some(HashMap::new()),
                    "String" => {
                        key.clear();
                        value = None;
                    }
                    _ => {}
                }
                buf.clear();
                path.push(name);
            }
            Event::Empty(e) => {
                if e.local_name().as_ref() == "Value" {
                    value = Some(Zeroizing::new(String::new()));
                }
            }
            Event::Text(t) => {
                if buf.len() < MAX_FIELD * 2 {
                    buf.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if buf.len() < MAX_FIELD * 2 {
                    buf.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(s) = resolve_reference(&r) {
                    buf.push_str(&s);
                }
            }
            Event::End(_) => {
                let Some(name) = path.pop() else { return Err(bad()) };
                let parent = path.last().map(String::as_str);
                match name.as_str() {
                    "Key" => key = buf.trim().to_owned(),
                    "Value" => value = Some(Zeroizing::new(buf.to_string())),
                    "String" => {
                        if let (Some(e), Some(v)) = (entry.as_mut(), value.take())
                            && history_depth == 0
                        {
                            e.insert(key.clone(), v);
                        }
                    }
                    "Name" if parent == Some("Group") => {
                        if let Some(g) = groups.last_mut() {
                            *g = buf.trim().to_owned();
                        }
                    }
                    "Entry" if history_depth == 0 => {
                        if let Some(fields) = entry.take() {
                            // Itens da lixeira não são importados. / Recycle bin items are not imported.
                            if groups.iter().any(|g| g == "Recycle Bin") || items.len() >= MAX_ITEMS {
                                skipped += 1;
                                continue;
                            }
                            let field = |k: &str| fields.get(k).map(|v| v.to_string()).unwrap_or_default();
                            let tag = groups.iter().skip(1).filter(|g| !g.is_empty()).cloned().collect::<Vec<_>>().join("/");
                            let raw = Raw {
                                kind: None,
                                title: field("Title"),
                                username: field("UserName"),
                                url: field("URL"),
                                password: Zeroizing::new(field("Password")),
                                notes: Zeroizing::new(field("Notes")),
                                tags: vec![tag],
                            };
                            match raw.into_item() {
                                Some(item) => items.push(item),
                                None => skipped += 1,
                            }
                        }
                    }
                    "History" => history_depth = history_depth.saturating_sub(1),
                    "Group" => {
                        groups.pop();
                    }
                    _ => {}
                }
                buf.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_keepass {
        return Err(Error::UnsupportedImport("XML não é do KeePass / XML is not from KeePass"));
    }
    Ok(Parsed { format: Format::KeePassXml, items, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME: &str = "name,url,username,password,note\n\
        banco.example,https://banco.example/login,ana,s3nha,\n\
        \"Fórum, o melhor\",https://forum.example,ana@x.com,\"p\"\"q,r\",\"linha1\nlinha2\"\n";
    const FIREFOX: &str = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\
        \"https://loja.example:8443\",\"bia\",\"x1\",,\"https://loja.example\",\"{a}\",\"1\",\"2\",\"3\"\n";
    const BITWARDEN_CSV: &str = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
        Trabalho,,login,Email,,,0,https://mail.example,ana,pw1,\n\
        ,,note,Wi-Fi,senha da rede,,0,,,,\n\
        ,,card,Cartão,,,0,,,,\n";
    const BITWARDEN_JSON: &str = r#"{"encrypted":false,"folders":[{"id":"f1","name":"Casa"}],
        "items":[{"type":1,"name":"Roteador","folderId":"f1","notes":null,"login":{"username":"admin","password":"pw2","uris":[{"uri":"http://192.168.0.1"}]}},
                 {"type":2,"name":"Nota","notes":"segredo"},
                 {"type":3,"name":"Cartão"}]}"#;
    const KEEPASS_XML: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<KeePassFile><Root><Group><Name>Database</Name>
  <Group><Name>Email</Name>
    <Entry>
      <String><Key>Title</Key><Value>Correio &amp; Cia</Value></String>
      <String><Key>UserName</Key><Value>ana</Value></String>
      <String><Key>Password</Key><Value ProtectInMemory="True">p&lt;3&#233;</Value></String>
      <String><Key>URL</Key><Value>https://mail.example</Value></String>
      <String><Key>Notes</Key><Value/></String>
      <History><Entry><String><Key>Title</Key><Value>Antigo</Value></String><String><Key>Password</Key><Value>velha</Value></String></Entry></History>
    </Entry>
  </Group>
  <Group><Name>Recycle Bin</Name><Entry><String><Key>Title</Key><Value>Apagado</Value></String><String><Key>Password</Key><Value>x</Value></String></Entry></Group>
</Group></Root></KeePassFile>"#;

    #[test]
    fn chrome_with_quotes_commas_and_newlines() {
        let p = parse(CHROME.as_bytes()).unwrap();
        assert_eq!(p.format, Format::ChromeCsv);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[1].title, "Fórum, o melhor");
        assert_eq!(p.items[1].password.as_deref(), Some("p\"q,r"));
        assert_eq!(p.items[1].notes.as_deref(), Some("linha1\nlinha2"));
    }

    #[test]
    fn firefox_uses_host_as_title() {
        let p = parse(FIREFOX.as_bytes()).unwrap();
        assert_eq!(p.format, Format::FirefoxCsv);
        assert_eq!(p.items[0].title, "loja.example");
        assert_eq!(p.items[0].username, "bia");
    }

    #[test]
    fn bitwarden_csv_skips_cards() {
        let p = parse(BITWARDEN_CSV.as_bytes()).unwrap();
        assert_eq!(p.format, Format::BitwardenCsv);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.skipped, 1);
        assert_eq!(p.items[0].tags, vec!["Trabalho"]);
        assert_eq!(p.items[1].kind, ItemKind::SecureNote);
    }

    #[test]
    fn bitwarden_json_and_encrypted_rejected() {
        let p = parse(BITWARDEN_JSON.as_bytes()).unwrap();
        assert_eq!(p.format, Format::BitwardenJson);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.skipped, 1);
        assert_eq!(p.items[0].tags, vec!["Casa"]);
        assert_eq!(p.items[0].url, "http://192.168.0.1");
        assert!(matches!(parse(br#"{"encrypted":true,"items":[]}"#), Err(Error::UnsupportedImport(_))));
    }

    #[test]
    fn keepass_xml_entities_history_and_recycle_bin() {
        let p = parse(KEEPASS_XML.as_bytes()).unwrap();
        assert_eq!(p.format, Format::KeePassXml);
        assert_eq!(p.items.len(), 1, "histórico e lixeira não entram / history and bin excluded");
        assert_eq!(p.items[0].title, "Correio & Cia");
        assert_eq!(p.items[0].password.as_deref(), Some("p<3é"));
        assert_eq!(p.items[0].tags, vec!["Email"]);
    }

    #[test]
    fn xml_external_entities_are_not_expanded() {
        let xxe = r#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///c:/windows/win.ini">]>
<KeePassFile><Root><Group><Name>R</Name><Entry><String><Key>Title</Key><Value>&xxe;</Value></String>
<String><Key>Password</Key><Value>p</Value></String></Entry></Group></Root></KeePassFile>"#;
        let p = parse(xxe.as_bytes()).unwrap();
        assert!(!p.items.iter().any(|i| i.title.contains("fonts") || i.title.contains("[")));
    }

    #[test]
    fn sigilvault_csv_roundtrip_fields() {
        let csv = "kind,title,username,url,password,notes,tags\nsecure_note,Nota,,,,texto,\"a, b\"\nlogin,Site,u,https://s.example,pw,,\n";
        let p = parse(csv.as_bytes()).unwrap();
        assert_eq!(p.format, Format::SigilVaultCsv);
        assert_eq!(p.items[0].kind, ItemKind::SecureNote);
        assert_eq!(p.items[0].tags, vec!["a", "b"]);
    }

    #[test]
    fn unknown_and_empty_files() {
        assert!(matches!(parse(b""), Err(Error::UnsupportedImport(_))));
        assert!(matches!(parse(b"a,b,c\n1,2,3\n"), Err(Error::UnsupportedImport(_))));
        assert!(matches!(parse(b"<html></html>"), Err(Error::UnsupportedImport(_))));
        assert!(parse(b"{nao e json").is_err());
    }

    #[test]
    fn host_extraction() {
        assert_eq!(host_of("https://user:pw@mail.example:8080/x?y"), "mail.example");
        assert_eq!(host_of("mail.example/login"), "mail.example");
        assert_eq!(host_of(""), "");
    }

    /// Gerador pseudoaleatório simples e determinístico para o fuzzing.
    /// Simple deterministic pseudo-random generator for fuzzing.
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// Fuzzing: milhares de arquivos corrompidos (bytes trocados, apagados, repetidos e cortados)
    /// não podem causar pânico; e nunca saem mais itens do que entradas razoáveis.
    /// Fuzzing: thousands of corrupted files (bytes flipped, deleted, duplicated and truncated)
    /// must never panic; and never yield more items than plausible entries.
    #[test]
    fn fuzz_malformed_inputs_never_panic() {
        let seeds = [CHROME, FIREFOX, BITWARDEN_CSV, BITWARDEN_JSON, KEEPASS_XML];
        let mut rng = XorShift(0x5167_11fa_u64);
        let specials = b"\",\n\r<>&;#{}[]:\\\0\xff\xfe";
        for seed in seeds {
            for _ in 0..1500 {
                let mut data = seed.as_bytes().to_vec();
                for _ in 0..(1 + rng.next() % 8) {
                    if data.is_empty() {
                        break;
                    }
                    let pos = (rng.next() as usize) % data.len();
                    match rng.next() % 5 {
                        0 => data[pos] = specials[(rng.next() as usize) % specials.len()],
                        1 => {
                            data.remove(pos);
                        }
                        2 => data.insert(pos, specials[(rng.next() as usize) % specials.len()]),
                        3 => data.truncate(pos),
                        _ => {
                            let end = (pos + (rng.next() as usize % 40)).min(data.len());
                            let chunk = data[pos..end].to_vec();
                            data.splice(pos..pos, chunk);
                        }
                    }
                }
                if let Ok(p) = parse(&data) {
                    assert!(p.items.len() <= data.len());
                }
            }
        }
        // Bytes totalmente aleatórios. / Fully random bytes.
        for _ in 0..2000 {
            let len = (rng.next() % 300) as usize;
            let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            let _ = parse(&data);
        }
    }

    #[test]
    fn deep_xml_is_rejected_not_overflowing() {
        let deep = format!("<KeePassFile>{}{}</KeePassFile>", "<a>".repeat(10_000), "</a>".repeat(10_000));
        assert!(parse(deep.as_bytes()).is_err());
    }

    #[test]
    fn huge_fields_are_skipped() {
        let big = "x".repeat(MAX_FIELD + 1);
        let csv = format!("name,url,username,password\nSite,https://a.example,u,{big}\nOk,https://b.example,u,p\n");
        let p = parse(csv.as_bytes()).unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.skipped, 1);
    }
}
