//! Bloqueio por eventos do sistema (tela bloqueada, suspensão, fim da sessão). Ainda não
//! implementado no Linux.
//! Lock on system events (screen lock, suspend, session end). Not implemented on Linux yet.

/// Começa a vigiar. `on_lock` roda a cada evento que deve trancar o cofre.
/// Starts watching. `on_lock` runs on every event that must lock the vault.
pub fn start(_on_lock: impl Fn() + Send + Sync + 'static) {}
