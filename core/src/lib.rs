//! # sigilvault-core
//!
//! Núcleo do SigilVault: o único componente que descriptografa senhas.
//! SigilVault core: the only component that decrypts passwords.

pub mod crypto;
pub mod error;
pub mod generator;
mod hex;
pub mod recovery;

pub use error::{Error, Result};
