//! O cofre: criar, abrir, recuperar, salvar e gerenciar itens.
//! The vault: create, open, recover, save and manage items.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sigilvault_locked::LockedKey;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{self, KEY_LEN, KdfParams, SALT_LEN, Sealed, SecretKey};
use crate::error::{Error, Result};
use crate::format::{self, Header, PasswordSlot, RecoverySlot, VAULT_ID_LEN};
use crate::recovery::{self, CODE_COUNT, CODE_ID_LEN, RecoveryCode};
use crate::strength;

mod health;
mod transfer;

pub use health::{HealthEntry, HealthReport};
pub use transfer::{BackupInfo, ImportReport};

const INFO_FIELD_KEY: &[u8] = b"sigilvault v1 item field key";
const FIELD_PASSWORD: &str = "password";
const FIELD_NOTES: &str = "notes";
/// Rótulo (AAD) das senhas antigas no histórico. / Label (AAD) of old passwords in the history.
const FIELD_HISTORY: &str = "history";
/// Quantas senhas anteriores cada item guarda. / How many previous passwords each item keeps.
pub const MAX_HISTORY: usize = 10;

/// Identificador aleatório de um item (128 bits).
/// Random item identifier (128 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(#[serde(with = "crate::hex::array")] [u8; 16]);

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&crate::hex::encode(&self.0))
    }
}

impl std::str::FromStr for ItemId {
    type Err = Error;

    /// Lê o ID em hexadecimal (como vem da interface).
    /// Parses the hex ID (as it comes from the UI).
    fn from_str(s: &str) -> Result<Self> {
        let bytes = crate::hex::decode(s).ok_or(Error::ItemNotFound)?;
        Ok(Self(bytes.try_into().map_err(|_| Error::ItemNotFound)?))
    }
}

/// Tipo de item. / Item kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Login de site ou app. / Website or app login.
    #[default]
    Login,
    /// Nota segura. / Secure note.
    SecureNote,
    /// Programa do computador (Auto-Type): identificado pelo executável, não pelo título.
    /// Desktop program (Auto-Type): identified by its executable, not by the title.
    Program,
}

/// Senha anterior de um item (cifrada). / Previous password of an item (encrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryEntry {
    password: Sealed,
    changed_at: u64,
}

/// Item guardado no cofre. Senha e notas ficam cifradas até alguém pedir para ver.
/// Item stored in the vault. Password and notes stay encrypted until someone asks to see them.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct Item {
    #[zeroize(skip)]
    id: ItemId,
    #[zeroize(skip)]
    kind: ItemKind,
    title: String,
    username: String,
    url: String,
    tags: Vec<String>,
    #[zeroize(skip)]
    password: Option<Sealed>,
    #[zeroize(skip)]
    notes: Option<Sealed>,
    created_at: u64,
    updated_at: u64,
    /// Aceitar subdomínios do endereço salvo (desligado por padrão). / Accept subdomains of the saved address (off by default).
    #[serde(default)]
    match_subdomains: bool,
    /// Senhas anteriores, da mais antiga para a mais nova. / Previous passwords, oldest to newest.
    #[zeroize(skip)]
    #[serde(default)]
    history: Vec<HistoryEntry>,
    /// Programa: caminho completo do executável. / Program: full executable path.
    #[serde(default)]
    app_path: String,
    /// Programa: SHA-256 do certificado de quem assinou o executável (hex), se assinado.
    /// Program: SHA-256 of the executable signer's certificate (hex), if signed.
    #[serde(default)]
    app_signer: Option<String>,
}

impl Item {
    /// Identificador. / Identifier.
    pub fn id(&self) -> ItemId {
        self.id
    }
    /// Tipo. / Kind.
    pub fn kind(&self) -> ItemKind {
        self.kind
    }
    /// Título. / Title.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Usuário ou e-mail do login. / Login username or e-mail.
    pub fn username(&self) -> &str {
        &self.username
    }
    /// Endereço do site. / Website address.
    pub fn url(&self) -> &str {
        &self.url
    }
    /// Etiquetas. / Tags.
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    /// Tem senha guardada? / Has a stored password?
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }
    /// Tem notas guardadas? / Has stored notes?
    pub fn has_notes(&self) -> bool {
        self.notes.is_some()
    }
    /// Criado em (segundos Unix). / Created at (Unix seconds).
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
    /// Alterado em (segundos Unix). / Updated at (Unix seconds).
    pub fn updated_at(&self) -> u64 {
        self.updated_at
    }
    /// Aceita subdomínios do endereço salvo? / Accepts subdomains of the saved address?
    pub fn match_subdomains(&self) -> bool {
        self.match_subdomains
    }
    /// Quantas senhas anteriores estão guardadas. / How many previous passwords are stored.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    /// Programa: caminho do executável. / Program: executable path.
    pub fn app_path(&self) -> &str {
        &self.app_path
    }
    /// Programa: assinatura esperada (hex). / Program: expected signature (hex).
    pub fn app_signer(&self) -> Option<&str> {
        self.app_signer.as_deref()
    }
    /// Origem do endereço salvo, se for um site válido. / Origin of the saved address, if it is a valid site.
    pub fn origin(&self) -> Option<crate::Origin> {
        crate::Origin::parse(&self.url)
    }
}

/// Dados para criar um item. / Data to create an item.
#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub struct NewItem {
    /// Tipo. / Kind.
    #[zeroize(skip)]
    pub kind: ItemKind,
    /// Título (obrigatório). / Title (required).
    pub title: String,
    /// Usuário. / Username.
    pub username: String,
    /// Endereço. / Address.
    pub url: String,
    /// Etiquetas. / Tags.
    pub tags: Vec<String>,
    /// Senha. / Password.
    pub password: Option<String>,
    /// Notas. / Notes.
    pub notes: Option<String>,
    /// Aceitar subdomínios. / Accept subdomains.
    #[zeroize(skip)]
    pub match_subdomains: bool,
    /// Programa: caminho do executável. / Program: executable path.
    pub app_path: String,
    /// Programa: assinatura (hex). / Program: signature (hex).
    pub app_signer: Option<String>,
}

impl NewItem {
    /// Atalho para um login. / Shortcut for a login.
    pub fn login(title: &str, username: &str, url: &str, password: &str) -> Self {
        let mut item = Self::default();
        item.title = title.into();
        item.username = username.into();
        item.url = url.into();
        item.password = Some(password.into());
        item
    }

    /// Atalho para uma nota segura. / Shortcut for a secure note.
    pub fn note(title: &str, notes: &str) -> Self {
        let mut item = Self::default();
        item.kind = ItemKind::SecureNote;
        item.title = title.into();
        item.notes = Some(notes.into());
        item
    }
}

/// Alterações num item. Em `password` e `notes`: `None` mantém, `Some("")` remove, `Some(v)` troca.
/// Changes to an item. For `password` and `notes`: `None` keeps, `Some("")` removes, `Some(v)` replaces.
#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub struct ItemUpdate {
    /// Título (obrigatório). / Title (required).
    pub title: String,
    /// Usuário. / Username.
    pub username: String,
    /// Endereço. / Address.
    pub url: String,
    /// Etiquetas. / Tags.
    pub tags: Vec<String>,
    /// Senha nova, se mudou. / New password, if changed.
    pub password: Option<String>,
    /// Notas novas, se mudaram. / New notes, if changed.
    pub notes: Option<String>,
    /// Aceitar subdomínios. / Accept subdomains.
    #[zeroize(skip)]
    pub match_subdomains: bool,
    /// Programa: caminho do executável. / Program: executable path.
    pub app_path: String,
    /// Programa: assinatura (hex). / Program: signature (hex).
    pub app_signer: Option<String>,
}

/// Chave pública de um código, guardada no corpo cifrado.
/// A code's public key, stored in the encrypted body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPublic {
    #[serde(with = "crate::hex::array")]
    id: [u8; CODE_ID_LEN],
    #[serde(with = "crate::hex::array")]
    public_key: [u8; 32],
}

/// Conteúdo cifrado do cofre. / Encrypted vault contents.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct Body {
    username: String,
    #[zeroize(skip)]
    recovery_keys: Vec<RecoveryPublic>,
    items: Vec<Item>,
    /// Chave dos backups locais. Fica só aqui dentro: backups não têm slots, então só quem abre
    /// o cofre atual consegue restaurá-los.
    /// Local backup key. It lives only here: backups have no slots, so only someone who opens
    /// the current vault can restore them.
    #[serde(default)]
    backup_key: Option<BackupKey>,
}

/// Chave de backup (256 bits), em memória travada. / Backup key (256 bits), in locked memory.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
struct BackupKey(LockedKey);

impl BackupKey {
    fn random() -> Result<Self> {
        Ok(Self(lock(crypto::random_key()?)))
    }
}

impl Serialize for BackupKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let hex = Zeroizing::new(crate::hex::encode(&*self.0));
        s.serialize_str(&hex)
    }
}

impl<'de> Deserialize<'de> for BackupKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        let hex = Zeroizing::new(String::deserialize(d)?);
        let bytes = Zeroizing::new(crate::hex::decode(&hex).ok_or_else(|| D::Error::custom("hex inválido / invalid hex"))?);
        let key: &[u8; KEY_LEN] = bytes.as_slice().try_into().map_err(|_| D::Error::custom("tamanho errado / wrong length"))?;
        Ok(Self(LockedKey::new(key)))
    }
}

/// Move uma chave para memória travada (a cópia temporária é apagada ao sair).
/// Moves a key into locked memory (the temporary copy is wiped on drop).
fn lock(key: SecretKey) -> LockedKey {
    LockedKey::new(&key)
}

/// Resultado de abrir o cofre com um código de recuperação.
/// Result of opening the vault with a recovery code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryOutcome {
    /// Nome de usuário do cofre, para mostrar a quem esqueceu.
    /// Vault username, to show someone who forgot it.
    pub username: String,
    /// Códigos que ainda valem ("restam 9"). / Codes still valid ("9 left").
    pub remaining_codes: usize,
}

/// Cofre desbloqueado. Ao sair de escopo, as chaves e os dados são apagados da memória.
/// Unlocked vault. When dropped, keys and data are wiped from memory.
pub struct Vault {
    vault_id: [u8; VAULT_ID_LEN],
    revision: u64,
    recovery_salt: [u8; SALT_LEN],
    /// Chaves em memória travada (fora do arquivo de paginação).
    /// Keys in locked memory (kept out of the page file).
    vault_key: LockedKey,
    field_key: LockedKey,
    password_slot: PasswordSlot,
    recovery_slots: Vec<RecoverySlot>,
    body: Body,
}

impl fmt::Debug for Vault {
    /// Nunca mostra chaves nem dados. / Never shows keys or data.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault").field("revision", &self.revision).finish_non_exhaustive()
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn derive_field_key(vault_key: &[u8; KEY_LEN], vault_id: &[u8; VAULT_ID_LEN]) -> Result<SecretKey> {
    crypto::hkdf::<KEY_LEN>(vault_key, vault_id, INFO_FIELD_KEY)
}

/// AAD de um campo: amarra o texto cifrado ao cofre, ao item e ao nome do campo.
/// Field AAD: binds the ciphertext to the vault, the item and the field name.
fn field_aad(vault_id: &[u8; VAULT_ID_LEN], item: &ItemId, field: &str) -> Vec<u8> {
    let mut aad = b"sigilvault v1 field".to_vec();
    aad.extend_from_slice(vault_id);
    aad.extend_from_slice(&item.0);
    aad.extend_from_slice(field.as_bytes());
    aad
}

fn unwrap_key(kek: &[u8; KEY_LEN], wrapped: &Sealed, aad: &[u8]) -> Result<SecretKey> {
    let plain = crypto::open(kek, &wrapped.nonce, &wrapped.ciphertext, aad).map_err(|_| Error::InvalidCredentials)?;
    let key: [u8; KEY_LEN] = plain.as_slice().try_into().map_err(|_| Error::InvalidCredentials)?;
    Ok(Zeroizing::new(key))
}

fn make_password_slot(
    vault_id: &[u8; VAULT_ID_LEN],
    vault_key: &[u8; KEY_LEN],
    password: &str,
    kdf: KdfParams,
) -> Result<PasswordSlot> {
    let salt: [u8; SALT_LEN] = crypto::random_array()?;
    let kek = crypto::derive_password_key(strength::normalize(password).as_bytes(), &salt, &kdf)?;
    let wrapped = crypto::seal(&kek, vault_key, &format::password_slot_aad(vault_id, &kdf, &salt))?;
    Ok(PasswordSlot { kdf, salt, wrapped })
}

fn make_recovery_slots(
    vault_id: &[u8; VAULT_ID_LEN],
    vault_key: &[u8; KEY_LEN],
    keys: &[RecoveryPublic],
) -> Result<Vec<RecoverySlot>> {
    keys.iter()
        .map(|k| {
            let w = recovery::wrap_for_code(&k.public_key, vault_key, &format::recovery_slot_aad(vault_id, &k.id))?;
            Ok(RecoverySlot { id: k.id, ephemeral_public: w.ephemeral_public, wrapped: w.sealed })
        })
        .collect()
}

fn new_codes(recovery_salt: &[u8; SALT_LEN]) -> Result<(Vec<RecoveryCode>, Vec<RecoveryPublic>)> {
    let mut codes = Vec::with_capacity(CODE_COUNT);
    let mut publics = Vec::with_capacity(CODE_COUNT);
    for _ in 0..CODE_COUNT {
        let code = RecoveryCode::generate()?;
        let keys = code.derive(recovery_salt)?;
        publics.push(RecoveryPublic { id: keys.id, public_key: keys.public() });
        codes.push(code);
    }
    Ok((codes, publics))
}

impl Vault {
    /// Cria um cofre novo. Devolve o cofre e os 10 códigos de recuperação, que devem ser
    /// mostrados ao usuário uma única vez.
    /// Creates a new vault. Returns the vault and the 10 recovery codes, which must be
    /// shown to the user exactly once.
    pub fn create(username: &str, master_password: &str, kdf: KdfParams) -> Result<(Self, Vec<RecoveryCode>)> {
        let username = username.trim();
        if username.is_empty() {
            return Err(Error::InvalidInput("nome de usuário vazio / empty username"));
        }
        kdf.validate()?;
        strength::check_master_password(master_password, username)?;

        let vault_id: [u8; VAULT_ID_LEN] = crypto::random_array()?;
        let recovery_salt: [u8; SALT_LEN] = crypto::random_array()?;
        let (codes, publics) = new_codes(&recovery_salt)?;
        let vault_key = crypto::random_key()?;
        let vault = Self {
            vault_id,
            revision: 0,
            recovery_salt,
            field_key: lock(derive_field_key(&vault_key, &vault_id)?),
            password_slot: make_password_slot(&vault_id, &vault_key, master_password, kdf)?,
            recovery_slots: make_recovery_slots(&vault_id, &vault_key, &publics)?,
            vault_key: lock(vault_key),
            body: Body {
                username: username.to_owned(),
                recovery_keys: publics,
                items: Vec::new(),
                backup_key: Some(BackupKey::random()?),
            },
        };
        Ok((vault, codes))
    }

    /// Abre o cofre com a senha mestra.
    /// Opens the vault with the master password.
    pub fn open(bytes: &[u8], master_password: &str) -> Result<Self> {
        let parsed = format::parse(bytes)?;
        let h = &parsed.header;
        let slot = &h.password_slot;
        let kek = crypto::derive_password_key(strength::normalize(master_password).as_bytes(), &slot.salt, &slot.kdf)?;
        let vault_key = unwrap_key(&kek, &slot.wrapped, &format::password_slot_aad(&h.vault_id, &slot.kdf, &slot.salt))?;
        Self::decrypt(parsed, vault_key)
    }

    /// Abre o cofre com um código de recuperação e define uma senha mestra nova.
    /// O código é destruído, a chave do cofre é trocada e tudo é recriptografado.
    /// Salve o cofre logo em seguida.
    /// Opens the vault with a recovery code and sets a new master password.
    /// The code is destroyed, the vault key is rotated and everything is re-encrypted.
    /// Save the vault right afterwards.
    pub fn recover(bytes: &[u8], code: &str, new_master_password: &str) -> Result<(Self, RecoveryOutcome)> {
        let code = RecoveryCode::parse(code)?;
        let parsed = format::parse(bytes)?;
        let h = &parsed.header;
        let keys = code.derive(&h.recovery_salt)?;
        // O identificador acha o slot certo sem testar os 10. Código já usado = slot inexistente.
        // The identifier finds the right slot without trying all 10. Used code = missing slot.
        let slot = h.recovery_slots.iter().find(|s| s.id == keys.id).ok_or(Error::InvalidCredentials)?;
        let aad = format::recovery_slot_aad(&h.vault_id, &slot.id);
        let vault_key = recovery::unwrap_with_code(&keys, &slot.ephemeral_public, &slot.wrapped, &aad)?;
        let mut vault = Self::decrypt(parsed, vault_key)?;

        strength::check_master_password(new_master_password, &vault.body.username)?;
        let remaining: Vec<RecoveryPublic> =
            vault.body.recovery_keys.iter().filter(|k| k.id != keys.id).cloned().collect();
        let kdf = vault.password_slot.kdf;
        vault.rotate(new_master_password, kdf, remaining)?;

        let outcome = RecoveryOutcome {
            username: vault.body.username.clone(),
            remaining_codes: vault.remaining_recovery_codes(),
        };
        Ok((vault, outcome))
    }

    /// Confere se os bytes parecem um cofre SigilVault válido (estrutura e parâmetros), sem abrir.
    /// Checks whether the bytes look like a valid SigilVault vault (structure and parameters), without opening.
    pub fn check_file(bytes: &[u8]) -> Result<()> {
        format::parse(bytes).map(drop)
    }

    /// Lê um arquivo e abre com a senha mestra.
    /// Reads a file and opens it with the master password.
    pub fn open_file(path: impl AsRef<Path>, master_password: &str) -> Result<Self> {
        Self::open(&read_vault_file(path.as_ref())?, master_password)
    }

    fn decrypt(parsed: format::ParsedFile<'_>, vault_key: SecretKey) -> Result<Self> {
        let h = parsed.header;
        // Se o cabeçalho ou o corpo foram alterados, a verificação falha aqui.
        // If the header or body were altered, verification fails here.
        let plain = crypto::open(&vault_key, &h.body_nonce, parsed.body_ciphertext, parsed.aad)?;
        let mut body: Body = serde_json::from_slice(&plain).map_err(|_| Error::InvalidFormat("corpo / body"))?;
        if body.backup_key.is_none() {
            body.backup_key = Some(BackupKey::random()?);
        }
        // Os slots de código precisam bater com as chaves públicas guardadas no corpo.
        // Code slots must match the public keys stored in the body.
        let consistent = h.recovery_slots.len() == body.recovery_keys.len()
            && h.recovery_slots.iter().all(|s| body.recovery_keys.iter().any(|k| k.id == s.id));
        if !consistent {
            return Err(Error::Tampered);
        }
        Ok(Self {
            vault_id: h.vault_id,
            revision: h.revision,
            recovery_salt: h.recovery_salt,
            field_key: lock(derive_field_key(&vault_key, &h.vault_id)?),
            vault_key: lock(vault_key),
            password_slot: h.password_slot,
            recovery_slots: h.recovery_slots,
            body,
        })
    }

    /// Troca a chave do cofre: recriptografa todos os campos e refaz todos os slots.
    /// Tudo é calculado antes; o estado só muda se nada falhar.
    /// Rotates the vault key: re-encrypts every field and rebuilds every slot.
    /// Everything is computed first; state only changes if nothing fails.
    fn rotate(&mut self, password: &str, kdf: KdfParams, recovery_keys: Vec<RecoveryPublic>) -> Result<()> {
        let new_key = crypto::random_key()?;
        let new_field_key = derive_field_key(&new_key, &self.vault_id)?;

        let mut items = self.body.items.clone();
        for item in &mut items {
            for (field, slot) in [(FIELD_PASSWORD, &mut item.password), (FIELD_NOTES, &mut item.notes)] {
                if let Some(sealed) = slot.as_ref() {
                    let aad = field_aad(&self.vault_id, &item.id, field);
                    let plain = crypto::open(&self.field_key, &sealed.nonce, &sealed.ciphertext, &aad)?;
                    *slot = Some(crypto::seal(&new_field_key, &plain, &aad)?);
                }
            }
            let aad = field_aad(&self.vault_id, &item.id, FIELD_HISTORY);
            for entry in &mut item.history {
                let plain = crypto::open(&self.field_key, &entry.password.nonce, &entry.password.ciphertext, &aad)?;
                entry.password = crypto::seal(&new_field_key, &plain, &aad)?;
            }
        }
        let password_slot = make_password_slot(&self.vault_id, &new_key, password, kdf)?;
        let recovery_slots = make_recovery_slots(&self.vault_id, &new_key, &recovery_keys)?;

        self.body.items = items;
        self.body.recovery_keys = recovery_keys;
        self.password_slot = password_slot;
        self.recovery_slots = recovery_slots;
        self.vault_key = lock(new_key);
        self.field_key = lock(new_field_key);
        Ok(())
    }

    /// Confere a senha mestra atual (para ações sensíveis).
    /// Checks the current master password (for sensitive actions).
    pub fn verify_master_password(&self, master_password: &str) -> Result<()> {
        let slot = &self.password_slot;
        let kek = crypto::derive_password_key(strength::normalize(master_password).as_bytes(), &slot.salt, &slot.kdf)?;
        unwrap_key(&kek, &slot.wrapped, &format::password_slot_aad(&self.vault_id, &slot.kdf, &slot.salt)).map(drop)
    }

    /// Troca a senha mestra. Os códigos continuam valendo; a chave do cofre é trocada.
    /// Changes the master password. Codes stay valid; the vault key is rotated.
    pub fn change_master_password(&mut self, current: &str, new: &str) -> Result<()> {
        self.verify_master_password(current)?;
        strength::check_master_password(new, &self.body.username)?;
        let keys = self.body.recovery_keys.clone();
        self.rotate(new, self.password_slot.kdf, keys)
    }

    /// Gera 10 códigos novos e invalida os antigos. Exige a senha mestra.
    /// Generates 10 new codes and invalidates the old ones. Requires the master password.
    pub fn regenerate_recovery_codes(&mut self, master_password: &str) -> Result<Vec<RecoveryCode>> {
        self.verify_master_password(master_password)?;
        let (codes, publics) = new_codes(&self.recovery_salt)?;
        self.rotate(master_password, self.password_slot.kdf, publics)?;
        Ok(codes)
    }

    /// Serializa e cifra o cofre. Cada gravação aumenta a revisão (proteção contra voltar versão).
    /// Serializes and encrypts the vault. Each save bumps the revision (rollback protection).
    pub fn to_bytes(&mut self) -> Result<Vec<u8>> {
        let revision = self.revision.checked_add(1).ok_or(Error::InvalidFormat("revisão / revision"))?;
        let header = Header {
            vault_id: self.vault_id,
            revision,
            recovery_salt: self.recovery_salt,
            body_nonce: crypto::random_array()?,
            password_slot: self.password_slot.clone(),
            recovery_slots: self.recovery_slots.clone(),
        };
        let mut out = format::encode_header(&header)?;
        let plain = Zeroizing::new(serde_json::to_vec(&self.body).map_err(|_| Error::InvalidFormat("corpo / body"))?);
        let ciphertext = crypto::seal_with_nonce(&self.vault_key, &header.body_nonce, &plain, &out)?;
        out.extend_from_slice(&ciphertext);
        self.revision = revision;
        Ok(out)
    }

    /// Salva de forma atômica: grava num `.tmp`, força a ida ao disco e só então substitui.
    /// Saves atomically: writes a `.tmp`, flushes it to disk and only then replaces.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        atomic_write(path.as_ref(), &bytes)
    }

    /// Nome de usuário do cofre. / Vault username.
    pub fn username(&self) -> &str {
        &self.body.username
    }

    /// Revisão atual (aumenta a cada gravação). / Current revision (grows on every save).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// As chaves do cofre estão travadas na RAM (fora do arquivo de paginação)? Sempre `false`
    /// fora do Windows. / Are the vault keys locked in RAM (kept out of the page file)? Always
    /// `false` outside Windows.
    pub fn keys_locked(&self) -> bool {
        self.vault_key.is_locked() && self.field_key.is_locked() && self.body.backup_key.as_ref().is_none_or(|k| k.0.is_locked())
    }

    /// Quantos códigos de recuperação ainda valem. / How many recovery codes are still valid.
    pub fn remaining_recovery_codes(&self) -> usize {
        self.recovery_slots.len()
    }

    /// Parâmetros do Argon2id do slot da senha. / Password slot Argon2id parameters.
    pub fn kdf_params(&self) -> KdfParams {
        self.password_slot.kdf
    }

    /// Todos os itens (sem senhas em texto puro). / All items (no plaintext passwords).
    pub fn items(&self) -> &[Item] {
        &self.body.items
    }

    /// Um item pelo ID. / One item by ID.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.body.items.iter().find(|i| i.id == id)
    }

    /// Busca por título, usuário, endereço ou etiqueta (sem diferenciar maiúsculas).
    /// Searches title, username, address or tag (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&Item> {
        let q = query.trim().to_lowercase();
        self.body
            .items
            .iter()
            .filter(|i| {
                q.is_empty()
                    || [&i.title, &i.username, &i.url].iter().any(|f| f.to_lowercase().contains(&q))
                    || i.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    fn item_mut(&mut self, id: ItemId) -> Result<&mut Item> {
        self.body.items.iter_mut().find(|i| i.id == id).ok_or(Error::ItemNotFound)
    }

    fn seal_field(&self, id: &ItemId, field: &str, value: Option<&str>) -> Result<Option<Sealed>> {
        value.map(|v| crypto::seal(&self.field_key, v.as_bytes(), &field_aad(&self.vault_id, id, field))).transpose()
    }

    fn reveal_field(&self, id: ItemId, field: &str) -> Result<Option<Zeroizing<String>>> {
        let item = self.item(id).ok_or(Error::ItemNotFound)?;
        let sealed = match field {
            FIELD_PASSWORD => &item.password,
            _ => &item.notes,
        };
        let Some(sealed) = sealed else { return Ok(None) };
        let plain = crypto::open(&self.field_key, &sealed.nonce, &sealed.ciphertext, &field_aad(&self.vault_id, &id, field))?;
        let text = String::from_utf8(plain.to_vec()).map_err(|_| Error::Tampered)?;
        Ok(Some(Zeroizing::new(text)))
    }

    /// Adiciona um item e devolve o ID. / Adds an item and returns its ID.
    pub fn add_item(&mut self, new: NewItem) -> Result<ItemId> {
        if new.title.trim().is_empty() {
            return Err(Error::InvalidInput("título vazio / empty title"));
        }
        let id = ItemId(crypto::random_array()?);
        let ts = now();
        let item = Item {
            id,
            kind: new.kind,
            title: new.title.trim().to_owned(),
            username: new.username.clone(),
            url: new.url.clone(),
            tags: new.tags.clone(),
            // Texto vazio = sem senha/notas (mesma regra da edição). / Empty text = no password/notes (same rule as editing).
            password: self.seal_field(&id, FIELD_PASSWORD, new.password.as_deref().filter(|p| !p.is_empty()))?,
            notes: self.seal_field(&id, FIELD_NOTES, new.notes.as_deref().filter(|n| !n.is_empty()))?,
            created_at: ts,
            updated_at: ts,
            match_subdomains: new.match_subdomains,
            history: Vec::new(),
            app_path: new.app_path.trim().to_owned(),
            app_signer: new.app_signer.clone(),
        };
        self.body.items.push(item);
        Ok(id)
    }

    /// Mostra a senha de um item (só quando o usuário pede).
    /// Reveals an item's password (only when the user asks).
    pub fn reveal_password(&self, id: ItemId) -> Result<Option<Zeroizing<String>>> {
        self.reveal_field(id, FIELD_PASSWORD)
    }

    /// Mostra as notas de um item. / Reveals an item's notes.
    pub fn reveal_notes(&self, id: ItemId) -> Result<Option<Zeroizing<String>>> {
        self.reveal_field(id, FIELD_NOTES)
    }

    /// Troca a senha de um item. / Changes an item's password.
    pub fn set_password(&mut self, id: ItemId, password: &str) -> Result<()> {
        let archived = self.archive_if_changed(id, Some(password))?;
        let sealed = self.seal_field(&id, FIELD_PASSWORD, Some(password))?;
        let item = self.item_mut(id)?;
        item.password = sealed;
        push_history(item, archived);
        item.updated_at = now();
        Ok(())
    }

    /// Se a senha vai mudar, devolve a atual cifrada para o histórico.
    /// If the password is about to change, returns the current one encrypted for the history.
    fn archive_if_changed(&self, id: ItemId, new: Option<&str>) -> Result<Option<HistoryEntry>> {
        let Some(current) = self.reveal_password(id)? else { return Ok(None) };
        if new == Some(current.as_str()) {
            return Ok(None);
        }
        let sealed = crypto::seal(&self.field_key, current.as_bytes(), &field_aad(&self.vault_id, &id, FIELD_HISTORY))?;
        Ok(Some(HistoryEntry { password: sealed, changed_at: now() }))
    }

    /// Senhas anteriores de um item, da mais nova para a mais antiga (data da troca + senha).
    /// Previous passwords of an item, newest first (change date + password).
    pub fn reveal_history(&self, id: ItemId) -> Result<Vec<(u64, Zeroizing<String>)>> {
        let item = self.item(id).ok_or(Error::ItemNotFound)?;
        let aad = field_aad(&self.vault_id, &id, FIELD_HISTORY);
        item.history
            .iter()
            .rev()
            .map(|e| {
                let plain = crypto::open(&self.field_key, &e.password.nonce, &e.password.ciphertext, &aad)?;
                let text = String::from_utf8(plain.to_vec()).map_err(|_| Error::Tampered)?;
                Ok((e.changed_at, Zeroizing::new(text)))
            })
            .collect()
    }

    /// Programas salvos para este executável (comparação sem diferenciar maiúsculas e com
    /// barras normalizadas). / Programs saved for this executable (case-insensitive, normalized slashes).
    pub fn programs_for(&self, exe_path: &str) -> Vec<&Item> {
        let wanted = normalize_path(exe_path);
        if wanted.is_empty() {
            return Vec::new();
        }
        self.body
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Program && i.password.is_some() && normalize_path(&i.app_path) == wanted)
            .collect()
    }

    /// Logins que podem ser preenchidos nesta página (regra de [`crate::Origin::fills`]).
    /// Logins that may be filled on this page (rule of [`crate::Origin::fills`]).
    pub fn logins_for(&self, page: &crate::Origin) -> Vec<&Item> {
        self.body
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Login && i.password.is_some())
            .filter(|i| i.origin().is_some_and(|o| o.fills(page, i.match_subdomains)))
            .collect()
    }

    /// Remove um item. / Removes an item.
    pub fn remove_item(&mut self, id: ItemId) -> Result<()> {
        let pos = self.body.items.iter().position(|i| i.id == id).ok_or(Error::ItemNotFound)?;
        self.body.items.remove(pos);
        Ok(())
    }

    /// Edita um item. Tudo é validado e cifrado antes de mudar o estado.
    /// Edits an item. Everything is validated and encrypted before state changes.
    pub fn update_item(&mut self, id: ItemId, update: ItemUpdate) -> Result<()> {
        if update.title.trim().is_empty() {
            return Err(Error::InvalidInput("título vazio / empty title"));
        }
        self.item(id).ok_or(Error::ItemNotFound)?;
        let reseal = |field: &str, value: &Option<String>| -> Result<Option<Option<Sealed>>> {
            match value.as_deref() {
                None => Ok(None),
                Some("") => Ok(Some(None)),
                Some(v) => Ok(Some(self.seal_field(&id, field, Some(v))?)),
            }
        };
        let password = reseal(FIELD_PASSWORD, &update.password)?;
        let notes = reseal(FIELD_NOTES, &update.notes)?;
        let archived = match update.password.as_deref() {
            None => None,
            Some(new) => self.archive_if_changed(id, (!new.is_empty()).then_some(new))?,
        };

        let item = self.item_mut(id)?;
        item.title = update.title.trim().to_owned();
        item.username.clone_from(&update.username);
        item.url.clone_from(&update.url);
        item.tags.clone_from(&update.tags);
        if let Some(p) = password {
            item.password = p;
        }
        push_history(item, archived);
        item.match_subdomains = update.match_subdomains;
        item.app_path = update.app_path.trim().to_owned();
        item.app_signer.clone_from(&update.app_signer);
        if let Some(n) = notes {
            item.notes = n;
        }
        item.updated_at = now();
        Ok(())
    }

    /// Identificador público do cofre (hexadecimal), usado pelo app para o contador de versão.
    /// Public vault identifier (hex), used by the app for the version counter.
    pub fn vault_id(&self) -> String {
        crate::hex::encode(&self.vault_id)
    }

    #[cfg(test)]
    pub(crate) fn swap_passwords_for_test(&mut self, a: ItemId, b: ItemId) {
        let ia = self.body.items.iter().position(|i| i.id == a).unwrap();
        let ib = self.body.items.iter().position(|i| i.id == b).unwrap();
        let pa = self.body.items[ia].password.take();
        let pb = self.body.items[ib].password.take();
        self.body.items[ia].password = pb;
        self.body.items[ib].password = pa;
    }
}

/// Caminho de executável para comparar. Caminho de Windows (`C:\...` ou com barras invertidas) é
/// comparado sem diferenciar maiúsculas e com barras normalizadas; caminho do Linux é comparado
/// exatamente, porque lá maiúsculas e minúsculas são arquivos diferentes. Quem decide é o formato
/// do caminho, não o sistema onde o app roda: um cofre levado de um sistema para o outro continua
/// se comportando do mesmo jeito.
/// Executable path to compare. A Windows path (`C:\...` or with backslashes) is compared
/// case-insensitively with normalized slashes; a Linux path is compared exactly, because there
/// upper and lower case are different files. The path's format decides, not the system the app
/// runs on: a vault carried from one system to the other keeps behaving the same way.
pub fn normalize_path(path: &str) -> String {
    let path = path.trim();
    let drive = path.as_bytes().get(1) == Some(&b':') && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    if drive || path.contains('\\') {
        path.replace('/', "\\").to_lowercase()
    } else {
        path.to_owned()
    }
}

/// Guarda uma senha antiga, mantendo só as [`MAX_HISTORY`] mais novas.
/// Stores an old password, keeping only the [`MAX_HISTORY`] newest.
fn push_history(item: &mut Item, entry: Option<HistoryEntry>) {
    if let Some(e) = entry {
        item.history.push(e);
        let extra = item.history.len().saturating_sub(MAX_HISTORY);
        item.history.drain(..extra);
    }
}

fn read_vault_file(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > format::MAX_FILE_LEN as u64 {
        return Err(Error::InvalidFormat("arquivo grande demais / file too large"));
    }
    Ok(fs::read(path)?)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path.file_name().ok_or(Error::InvalidInput("caminho sem nome / path without name"))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        f.write_all(bytes)?;
        // sync_all = FlushFileBuffers no Windows. / sync_all = FlushFileBuffers on Windows.
        f.sync_all()?;
        drop(f);
        // rename = MoveFileExW(MOVEFILE_REPLACE_EXISTING) no Windows.
        // rename = MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows.
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_ignore_case_and_slashes_but_linux_paths_do_not() {
        assert_eq!(normalize_path(" C:/Jogos/Game.EXE "), r"c:\jogos\game.exe");
        assert_eq!(normalize_path(r"\\srv\Share\App.exe"), r"\\srv\share\app.exe");
        assert_eq!(normalize_path("/usr/bin/Firefox"), "/usr/bin/Firefox");
        assert_ne!(normalize_path("/usr/bin/App"), normalize_path("/usr/bin/app"));
        assert_eq!(normalize_path(""), "");
    }

    const PW: &str = "cavalo-trilho-nuvem-sete-farol-mesa";

    #[test]
    fn swapped_passwords_between_items_are_detected() {
        let (mut v, _) = Vault::create("ana", PW, KdfParams::MINIMUM).unwrap();
        let bank = v.add_item(NewItem::login("Banco", "ana", "https://banco.example", "senha-do-banco")).unwrap();
        let forum = v.add_item(NewItem::login("Fórum", "ana", "https://forum.example", "senha-do-forum")).unwrap();
        v.swap_passwords_for_test(bank, forum);
        // A AAD amarra a senha ao ID do item: trocada de lugar, ela não abre.
        // The AAD binds the password to the item ID: once moved, it won't open.
        assert!(matches!(v.reveal_password(bank), Err(Error::Tampered)));
        assert!(matches!(v.reveal_password(forum), Err(Error::Tampered)));
    }

    #[test]
    fn debug_hides_secrets() {
        let (v, codes) = Vault::create("ana", PW, KdfParams::MINIMUM).unwrap();
        let text = format!("{v:?} {codes:?}");
        assert!(!text.contains("ana"));
        assert!(!text.contains(&*codes[0].to_display_string()));
    }
}
