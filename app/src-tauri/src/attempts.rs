//! Limite de tentativas de desbloqueio com espera crescente. O contador fica salvo em disco
//! (fechar e abrir o app não zera) e o tempo é medido com relógio monotônico (`Instant`).
//! Unlock attempt limit with growing delay. The counter is saved to disk (restarting the app
//! does not reset it) and time is measured with a monotonic clock (`Instant`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::storage::atomic_write;

/// Erros permitidos antes de começar a esperar. / Mistakes allowed before waiting starts.
pub const FREE_ATTEMPTS: u32 = 3;
const BASE_DELAY: Duration = Duration::from_secs(5);
const MAX_DELAY: Duration = Duration::from_secs(15 * 60);

/// Espera depois de `failures` erros seguidos: 0, 0, 0, 5 s, 10 s, 20 s… até 15 min.
/// Delay after `failures` consecutive mistakes: 0, 0, 0, 5 s, 10 s, 20 s… up to 15 min.
pub fn delay_for(failures: u32) -> Duration {
    if failures < FREE_ATTEMPTS {
        return Duration::ZERO;
    }
    let exp = (failures - FREE_ATTEMPTS).min(16);
    BASE_DELAY.saturating_mul(1 << exp).min(MAX_DELAY)
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    failures: u32,
}

/// Controle de tentativas. / Attempt control.
pub struct Attempts {
    path: PathBuf,
    failures: u32,
    blocked_until: Option<Instant>,
}

impl Attempts {
    /// Carrega o contador. Se havia erros, a espera recomeça a contar agora (mais seguro).
    /// Loads the counter. If there were mistakes, the delay restarts now (safer).
    pub fn load(path: &Path, now: Instant) -> Self {
        let failures = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Stored>(&b).ok())
            .map(|s| s.failures)
            .unwrap_or(0);
        let delay = delay_for(failures);
        Self { path: path.to_owned(), failures, blocked_until: (!delay.is_zero()).then(|| now + delay) }
    }

    /// Tempo que ainda falta esperar (zero = pode tentar).
    /// Time left to wait (zero = may try).
    pub fn remaining(&self, now: Instant) -> Duration {
        self.blocked_until.map(|t| t.saturating_duration_since(now)).unwrap_or_default()
    }

    /// Registra um erro e devolve quanto tempo esperar.
    /// Records a mistake and returns how long to wait.
    pub fn record_failure(&mut self, now: Instant) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let delay = delay_for(self.failures);
        self.blocked_until = (!delay.is_zero()).then(|| now + delay);
        self.persist();
        delay
    }

    /// Zera o contador após um desbloqueio bem-sucedido.
    /// Resets the counter after a successful unlock.
    pub fn record_success(&mut self) {
        if self.failures != 0 {
            self.failures = 0;
            self.blocked_until = None;
            self.persist();
        }
    }

    fn persist(&self) {
        if let Ok(bytes) = serde_json::to_vec(&Stored { failures: self.failures }) {
            let _ = atomic_write(&self.path, &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_grows_and_caps() {
        assert_eq!(delay_for(0), Duration::ZERO);
        assert_eq!(delay_for(2), Duration::ZERO);
        assert_eq!(delay_for(3), Duration::from_secs(5));
        assert_eq!(delay_for(4), Duration::from_secs(10));
        assert_eq!(delay_for(5), Duration::from_secs(20));
        assert_eq!(delay_for(20), MAX_DELAY);
        assert_eq!(delay_for(u32::MAX), MAX_DELAY);
    }

    #[test]
    fn blocks_after_free_attempts_and_survives_restart() {
        let dir = crate::storage::test_dir("attempts");
        let path = dir.join("attempts.json");
        let t0 = Instant::now();
        let mut a = Attempts::load(&path, t0);
        assert_eq!(a.remaining(t0), Duration::ZERO);
        a.record_failure(t0);
        a.record_failure(t0);
        assert_eq!(a.remaining(t0), Duration::ZERO);
        assert_eq!(a.record_failure(t0), Duration::from_secs(5));
        assert_eq!(a.remaining(t0), Duration::from_secs(5));
        assert_eq!(a.remaining(t0 + Duration::from_secs(6)), Duration::ZERO);

        // Reabrir o app mantém o contador. / Reopening the app keeps the counter.
        let t1 = t0 + Duration::from_secs(60);
        let reopened = Attempts::load(&path, t1);
        assert_eq!(reopened.remaining(t1), Duration::from_secs(5));

        let mut a = reopened;
        a.record_success();
        assert_eq!(Attempts::load(&path, t1).remaining(t1), Duration::ZERO);
    }
}
