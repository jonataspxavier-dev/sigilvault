//! Exportação, backups locais e entrada de itens de fora (importação e mesclagem).
//! Export, local backups and items coming from outside (import and merge).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::{BackupKey, Body, FIELD_NOTES, FIELD_PASSWORD, Item, ItemId, ItemKind, NewItem, Vault, derive_field_key, make_password_slot, now};
use crate::backup::{self, BackupHeader};
use crate::crypto::{self, KdfParams, SALT_LEN};
use crate::error::{Error, Result};
use crate::format::VAULT_ID_LEN;
use crate::strength;

/// Item "portátil": campos em texto puro, só usado dentro de um invólucro cifrado
/// (backup ou cópia exportada). Apagado da memória ao sair de escopo.
/// "Portable" item: plaintext fields, only used inside an encrypted envelope
/// (backup or exported copy). Wiped from memory when dropped.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortableItem {
    #[zeroize(skip)]
    kind: ItemKind,
    title: String,
    username: String,
    url: String,
    tags: Vec<String>,
    password: Option<String>,
    notes: Option<String>,
    created_at: u64,
    updated_at: u64,
}

impl PortableItem {
    /// Impressão digital para achar duplicatas (sem guardar o texto).
    /// Fingerprint to find duplicates (without keeping the text).
    fn fingerprint(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        for part in [
            if self.kind == ItemKind::Login { "login" } else { "note" },
            self.title.trim(),
            self.username.trim(),
            self.url.trim(),
            self.password.as_deref().unwrap_or(""),
            self.notes.as_deref().unwrap_or(""),
        ] {
            h.update((part.len() as u64).to_le_bytes());
            h.update(part.as_bytes());
        }
        h.finalize().into()
    }

    fn from_new(item: &NewItem) -> Self {
        let ts = now();
        Self {
            kind: item.kind,
            title: item.title.trim().to_owned(),
            username: item.username.clone(),
            url: item.url.clone(),
            tags: item.tags.clone(),
            password: item.password.clone().filter(|p| !p.is_empty()),
            notes: item.notes.clone().filter(|n| !n.is_empty()),
            created_at: ts,
            updated_at: ts,
        }
    }
}

/// Conteúdo de um backup. / Backup content.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct BackupPayload {
    username: String,
    items: Vec<PortableItem>,
}

/// Resumo de um backup (para a lista na interface). / Backup summary (for the UI list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    /// Quando foi feito (segundos Unix). / When it was made (Unix seconds).
    pub created_at: u64,
    /// Revisão do cofre na hora. / Vault revision at the time.
    pub revision: u64,
    /// Quantos itens contém. / How many items it holds.
    pub item_count: usize,
}

/// Resultado de importar ou mesclar itens. / Result of importing or merging items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Itens adicionados. / Items added.
    pub added: usize,
    /// Itens ignorados por já existirem iguais. / Items skipped because identical ones exist.
    pub duplicates: usize,
}

impl Vault {
    fn backup_key(&self) -> Result<&[u8; 32]> {
        self.body.backup_key.as_ref().map(|k| &k.0).ok_or(Error::Crypto)
    }

    /// Converte os itens para a forma portátil (descriptografa senhas e notas).
    /// Converts items to portable form (decrypts passwords and notes).
    fn portable_items(&self) -> Result<Vec<PortableItem>> {
        self.body
            .items
            .iter()
            .map(|i| {
                Ok(PortableItem {
                    kind: i.kind,
                    title: i.title.clone(),
                    username: i.username.clone(),
                    url: i.url.clone(),
                    tags: i.tags.clone(),
                    password: self.reveal_password(i.id)?.map(|p| p.to_string()),
                    notes: self.reveal_notes(i.id)?.map(|n| n.to_string()),
                    created_at: i.created_at,
                    updated_at: i.updated_at,
                })
            })
            .collect()
    }

    /// Cria um item a partir da forma portátil, com ID novo. / Creates an item from portable form, with a new ID.
    fn item_from_portable(&self, p: &PortableItem) -> Result<Item> {
        let id = ItemId(crypto::random_array()?);
        Ok(Item {
            id,
            kind: p.kind,
            title: p.title.clone(),
            username: p.username.clone(),
            url: p.url.clone(),
            tags: p.tags.clone(),
            password: self.seal_field(&id, FIELD_PASSWORD, p.password.as_deref())?,
            notes: self.seal_field(&id, FIELD_NOTES, p.notes.as_deref())?,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
    }

    /// Adiciona itens ignorando os idênticos aos que já existem (ou repetidos no lote).
    /// Adds items, skipping those identical to existing ones (or repeated in the batch).
    fn add_portable(&mut self, incoming: &[PortableItem]) -> Result<ImportReport> {
        let mut seen: HashSet<[u8; 32]> = self.portable_items()?.iter().map(PortableItem::fingerprint).collect();
        let mut report = ImportReport::default();
        let mut new_items = Vec::new();
        for p in incoming {
            if p.title.trim().is_empty() {
                continue;
            }
            if seen.insert(p.fingerprint()) {
                new_items.push(self.item_from_portable(p)?);
                report.added += 1;
            } else {
                report.duplicates += 1;
            }
        }
        self.body.items.extend(new_items);
        Ok(report)
    }

    /// Importa itens lidos de outro gerenciador. / Imports items read from another manager.
    pub fn import_items(&mut self, items: &[NewItem]) -> Result<ImportReport> {
        let portable: Vec<PortableItem> = items.iter().map(PortableItem::from_new).collect();
        self.add_portable(&portable)
    }

    /// Traz os itens de outro cofre SigilVault já aberto. / Brings in the items of another opened SigilVault vault.
    pub fn merge_from(&mut self, other: &Vault) -> Result<ImportReport> {
        self.add_portable(&other.portable_items()?)
    }

    // ── Exportação / Export ────────────────────────────────────────

    /// Gera uma cópia com senha própria: cofre novo (chaves e ID novos), **sem** slots de códigos
    /// de recuperação e sem a chave dos backups deste PC.
    /// Generates a copy with its own password: a new vault (new keys and ID), **without** recovery
    /// code slots and without this PC's backup key.
    pub fn export_with_password(&self, password: &str, kdf: KdfParams) -> Result<Vec<u8>> {
        strength::check_master_password(password, &self.body.username)?;
        kdf.validate()?;
        let vault_id: [u8; VAULT_ID_LEN] = crypto::random_array()?;
        let vault_key = crypto::random_key()?;
        let mut copy = Vault {
            vault_id,
            revision: 0,
            recovery_salt: crypto::random_array::<SALT_LEN>()?,
            field_key: derive_field_key(&vault_key, &vault_id)?,
            password_slot: make_password_slot(&vault_id, &vault_key, password, kdf)?,
            recovery_slots: Vec::new(),
            vault_key,
            body: Body {
                username: self.body.username.clone(),
                recovery_keys: Vec::new(),
                items: Vec::new(),
                backup_key: Some(BackupKey::random()?),
            },
        };
        for p in &self.portable_items()? {
            let item = copy.item_from_portable(p)?;
            copy.body.items.push(item);
        }
        copy.to_bytes()
    }

    // ── Backups ────────────────────────────────────────────────────

    fn seal_backup(&self, key: &[u8; 32], payload: &BackupPayload, created_at: u64, revision: u64) -> Result<Vec<u8>> {
        let header = BackupHeader { vault_id: self.vault_id, created_at, revision, nonce: crypto::random_array()? };
        let mut out = backup::encode_header(&header)?;
        let plain = Zeroizing::new(serde_json::to_vec(payload).map_err(|_| Error::InvalidFormat("backup"))?);
        let ct = crypto::seal_with_nonce(key, &header.nonce, &plain, &out)?;
        out.extend_from_slice(&ct);
        Ok(out)
    }

    fn open_backup(&self, key: &[u8; 32], bytes: &[u8]) -> Result<(BackupHeader, BackupPayload)> {
        let (header, aad, ct) = backup::parse(bytes)?;
        if header.vault_id != self.vault_id {
            return Err(Error::InvalidFormat("backup de outro cofre / backup from another vault"));
        }
        let plain = crypto::open(key, &header.nonce, ct, aad)?;
        let payload = serde_json::from_slice(&plain).map_err(|_| Error::InvalidFormat("backup"))?;
        Ok((header, payload))
    }

    /// Cria um backup do estado atual. / Creates a backup of the current state.
    pub fn create_backup(&self) -> Result<Vec<u8>> {
        let payload = BackupPayload { username: self.body.username.clone(), items: self.portable_items()? };
        self.seal_backup(self.backup_key()?, &payload, now(), self.revision)
    }

    /// Lê o resumo de um backup (falha se não for deste cofre ou estiver adulterado).
    /// Reads a backup summary (fails if not from this vault or tampered).
    pub fn inspect_backup(&self, bytes: &[u8]) -> Result<BackupInfo> {
        let (header, payload) = self.open_backup(self.backup_key()?, bytes)?;
        Ok(BackupInfo { created_at: header.created_at, revision: header.revision, item_count: payload.items.len() })
    }

    /// Restaura um backup: os itens atuais são trocados pelos do backup.
    /// Restores a backup: current items are replaced by the backup's.
    pub fn restore_backup(&mut self, bytes: &[u8]) -> Result<()> {
        let (_, payload) = self.open_backup(self.backup_key()?, bytes)?;
        let items = payload.items.iter().map(|p| self.item_from_portable(p)).collect::<Result<Vec<_>>>()?;
        self.body.items = items;
        Ok(())
    }

    /// Troca a chave dos backups e recriptografa os backups dados. Backups ilegíveis viram `None`
    /// (o app deve apagá-los). Chame depois de trocar a senha, usar um código ou gerar códigos.
    /// Rotates the backup key and re-encrypts the given backups. Unreadable backups become `None`
    /// (the app should delete them). Call after changing the password, using a code or
    /// generating codes.
    pub fn rekey_backups(&mut self, backups: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        let old = *self.backup_key()?;
        let old = Zeroizing::new(old);
        let new_key = BackupKey::random()?;
        let mut out = Vec::with_capacity(backups.len());
        for bytes in backups {
            out.push(match self.open_backup(&old, bytes) {
                Ok((h, payload)) => Some(self.seal_backup(&new_key.0, &payload, h.created_at, h.revision)?),
                Err(_) => None,
            });
        }
        self.body.backup_key = Some(new_key);
        Ok(out)
    }

    // ── CSV ────────────────────────────────────────────────────────

    /// Exporta tudo em CSV **legível** (opção avançada; o app deve avisar e pedir a senha mestra).
    /// Exports everything as **readable** CSV (advanced option; the app must warn and ask for
    /// the master password).
    pub fn export_csv(&self) -> Result<Zeroizing<Vec<u8>>> {
        let mut w = csv::Writer::from_writer(Vec::new());
        let io = |_| Error::InvalidFormat("csv");
        w.write_record(crate::import::SIGILVAULT_CSV_HEADER).map_err(io)?;
        for p in self.portable_items()? {
            let kind = if p.kind == ItemKind::Login { "login" } else { "secure_note" };
            let tags = p.tags.join(", ");
            w.write_record([
                kind,
                &p.title,
                &p.username,
                &p.url,
                p.password.as_deref().unwrap_or(""),
                p.notes.as_deref().unwrap_or(""),
                &tags,
            ])
            .map_err(io)?;
        }
        Ok(Zeroizing::new(w.into_inner().map_err(|_| Error::InvalidFormat("csv"))?))
    }
}
