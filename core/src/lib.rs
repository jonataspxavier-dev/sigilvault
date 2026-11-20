//! # sigilvault-core
//!
//! Núcleo do SigilVault: o único componente que descriptografa senhas.
//! SigilVault core: the only component that decrypts passwords.
//!
//! ```no_run
//! use sigilvault_core::{KdfParams, NewItem, Vault};
//!
//! // Criar o cofre e mostrar os 10 códigos uma única vez.
//! // Create the vault and show the 10 codes exactly once.
//! let (mut vault, codes) = Vault::create("ana", "cavalo-trilho-nuvem-sete-farol-mesa", KdfParams::DEFAULT)?;
//! for code in &codes {
//!     println!("{}", *code.to_display_string());
//! }
//! vault.add_item(NewItem::login("Fórum", "ana", "https://forum.example", "s3nh@"))?;
//! vault.save("cofre.sigil")?;
//!
//! let vault = Vault::open_file("cofre.sigil", "cavalo-trilho-nuvem-sete-farol-mesa")?;
//! # Ok::<(), sigilvault_core::Error>(())
//! ```

mod backup;
pub mod crypto;
pub mod error;
mod format;
pub mod generator;
pub mod hex;
pub mod import;
pub mod recovery;
pub mod strength;
mod vault;

pub use crypto::KdfParams;
pub use error::{Error, Result};
pub use generator::{GeneratorOptions, generate};
pub use recovery::RecoveryCode;
pub use vault::{BackupInfo, ImportReport, Item, ItemId, ItemKind, ItemUpdate, NewItem, RecoveryOutcome, Vault};
