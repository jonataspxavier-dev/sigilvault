//! Relatório de saúde: senhas fracas, repetidas ou antigas. Tudo calculado aqui, sem rede.
//! Health report: weak, reused or old passwords. Everything computed here, no network.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use super::{Vault, ItemId};
use crate::crypto;
use crate::error::Result;
use crate::strength;

/// Nota mínima do zxcvbn (0 a 4) para uma senha de site não ser "fraca".
/// Minimum zxcvbn score (0 to 4) for a site password not to be "weak".
pub const WEAK_BELOW: u8 = 3;
/// Idade (dias) a partir da qual uma senha é "antiga". / Age (days) from which a password is "old".
pub const OLD_AFTER_DAYS: u64 = 365;

/// Problemas de um item. / Problems of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEntry {
    /// Item. / Item.
    pub id: ItemId,
    /// Nota do zxcvbn (0 a 4). / zxcvbn score (0 to 4).
    pub score: u8,
    /// Nota abaixo de [`WEAK_BELOW`]. / Score below [`WEAK_BELOW`].
    pub weak: bool,
    /// Quantos OUTROS itens usam a mesma senha (0 = única). / How many OTHER items use the same password (0 = unique).
    pub reused_with: usize,
    /// Grupo de senha repetida (mesmo número = mesma senha), para a interface agrupar sem ver a senha.
    /// Reused-password group (same number = same password), so the UI can group without seeing it.
    pub reuse_group: Option<usize>,
    /// Dias desde que a senha atual foi definida. / Days since the current password was set.
    pub age_days: u64,
    /// Mais antiga que [`OLD_AFTER_DAYS`]. / Older than [`OLD_AFTER_DAYS`].
    pub old: bool,
}

impl HealthEntry {
    /// Tem algum problema? / Has any problem?
    pub fn has_issue(&self) -> bool {
        self.weak || self.reused_with > 0 || self.old
    }
}

/// Resultado do relatório. / Report result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// Itens com senha que foram analisados. / Items with a password that were analyzed.
    pub checked: usize,
    /// Só os itens com algum problema, os mais graves primeiro. / Only items with a problem, worst first.
    pub entries: Vec<HealthEntry>,
}

impl Vault {
    /// Analisa as senhas guardadas. `now` em segundos Unix (o app passa o relógio; testes passam
    /// uma data fixa). As senhas são abertas uma de cada vez e apagadas logo em seguida; para achar
    /// repetidas compara-se SHA-256(sal aleatório deste relatório ‖ senha), descartado no fim.
    /// Analyzes stored passwords. `now` in Unix seconds (the app passes the clock; tests pass a
    /// fixed date). Passwords are opened one at a time and wiped right after; to find reused ones
    /// it compares SHA-256(this report's random salt ‖ password), discarded at the end.
    pub fn health_report(&self, now: u64) -> Result<HealthReport> {
        let salt: [u8; 32] = crypto::random_array()?;
        let mut rows = Vec::new();
        for item in &self.body.items {
            let Some(password) = self.reveal_password(item.id)? else { continue };
            let digest: [u8; 32] = Sha256::new().chain_update(salt).chain_update(password.as_bytes()).finalize().into();
            let score = strength::score(&password, &item.username);
            let set_at = item.history.last().map_or(item.created_at, |h| h.changed_at);
            let age_days = now.saturating_sub(set_at) / 86_400;
            rows.push((digest, HealthEntry {
                id: item.id,
                score,
                weak: score < WEAK_BELOW,
                reused_with: 0,
                reuse_group: None,
                age_days,
                old: age_days > OLD_AFTER_DAYS,
            }));
        }

        // Agrupa por resumo; grupos numerados na ordem em que aparecem.
        // Groups by digest; groups numbered in order of appearance.
        let mut counts: HashMap<[u8; 32], usize> = HashMap::new();
        for (d, _) in &rows {
            *counts.entry(*d).or_default() += 1;
        }
        let mut groups: HashMap<[u8; 32], usize> = HashMap::new();
        for (d, e) in &mut rows {
            let n = counts[d];
            if n > 1 {
                e.reused_with = n - 1;
                let next = groups.len();
                e.reuse_group = Some(*groups.entry(*d).or_insert(next));
            }
        }

        let checked = rows.len();
        let mut entries: Vec<HealthEntry> = rows.into_iter().map(|(_, e)| e).filter(HealthEntry::has_issue).collect();
        // Mais grave primeiro: repetida e fraca > fraca > repetida > antiga; depois a nota.
        // Worst first: reused and weak > weak > reused > old; then the score.
        entries.sort_by_key(|e| (std::cmp::Reverse((e.weak as u8) * 2 + (e.reused_with > 0) as u8), e.score, std::cmp::Reverse(e.age_days)));
        Ok(HealthReport { checked, entries })
    }
}
