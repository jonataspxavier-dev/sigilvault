//! Gerador de senhas aleatórias, sem viés.
//! Unbiased random password generator.
//!
//! A frase-senha (lista de 7.776 palavras) fica para uma etapa futura, quando houver uma lista
//! em português revisada.
//! Passphrases (7,776-word list) come later, once a reviewed Portuguese list is chosen.

use zeroize::Zeroizing;

use crate::crypto::fill_random;
use crate::error::{Error, Result};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
/// Caracteres ambíguos que podem ser excluídos: `l`, `1`, `O`, `0`.
/// Ambiguous characters that may be excluded: `l`, `1`, `O`, `0`.
pub const AMBIGUOUS: &str = "l1O0";

/// Tamanho máximo de senha. / Maximum password length.
pub const MAX_LENGTH: usize = 128;

/// Opções do gerador.
/// Generator options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorOptions {
    /// Quantidade de caracteres. / Number of characters.
    pub length: usize,
    /// Letras minúsculas. / Lowercase letters.
    pub lowercase: bool,
    /// Letras maiúsculas. / Uppercase letters.
    pub uppercase: bool,
    /// Números. / Digits.
    pub digits: bool,
    /// Símbolos ASCII. / ASCII symbols.
    pub symbols: bool,
    /// Excluir `l`, `1`, `O`, `0`. / Exclude `l`, `1`, `O`, `0`.
    pub exclude_ambiguous: bool,
}

impl Default for GeneratorOptions {
    /// Padrão: 24 caracteres com todos os tipos (muitos sites limitam a 64).
    /// Default: 24 characters with every type (many sites cap at 64).
    fn default() -> Self {
        Self { length: 24, lowercase: true, uppercase: true, digits: true, symbols: true, exclude_ambiguous: false }
    }
}

impl GeneratorOptions {
    /// Conjuntos de caracteres ativos, já sem os ambíguos se pedido.
    /// Active character sets, ambiguous ones removed if requested.
    fn classes(&self) -> Vec<Vec<char>> {
        [(self.lowercase, LOWER), (self.uppercase, UPPER), (self.digits, DIGITS), (self.symbols, SYMBOLS)]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, set)| {
                set.chars().filter(|c| !(self.exclude_ambiguous && AMBIGUOUS.contains(*c))).collect()
            })
            .collect()
    }

    fn validate(&self) -> Result<Vec<Vec<char>>> {
        let classes = self.classes();
        if classes.is_empty() {
            return Err(Error::InvalidGeneratorOptions("nenhum tipo de caractere / no character type"));
        }
        if self.length == 0 || self.length > MAX_LENGTH {
            return Err(Error::InvalidGeneratorOptions("tamanho fora de 1..=128 / length outside 1..=128"));
        }
        if self.length < classes.len() {
            return Err(Error::InvalidGeneratorOptions(
                "tamanho menor que o número de tipos / length shorter than number of types",
            ));
        }
        Ok(classes)
    }

    /// Entropia em bits, considerando exclusões e a exigência de um caractere de cada tipo
    /// (contagem exata por inclusão-exclusão).
    /// Entropy in bits, accounting for exclusions and the one-of-each-type rule
    /// (exact count via inclusion–exclusion).
    pub fn entropy_bits(&self) -> Result<f64> {
        let classes = self.validate()?;
        let sizes: Vec<f64> = classes.iter().map(|c| c.len() as f64).collect();
        let n: f64 = sizes.iter().sum();
        let len = self.length as i32;
        // Senhas válidas = Σ_{S ⊆ tipos} (-1)^|S| · (N − |S|)^L. Calculado em proporção de N^L.
        // Valid passwords = Σ_{S ⊆ types} (-1)^|S| · (N − |S|)^L. Computed as a fraction of N^L.
        let mut fraction = 0.0;
        for mask in 0u32..(1 << sizes.len()) {
            let removed: f64 = (0..sizes.len()).filter(|i| mask & (1 << i) != 0).map(|i| sizes[i]).sum();
            let sign = if mask.count_ones() % 2 == 0 { 1.0 } else { -1.0 };
            fraction += sign * ((n - removed) / n).powi(len);
        }
        Ok(self.length as f64 * n.log2() + fraction.log2())
    }
}

/// Sorteia um índice em `0..n` sem viés (rejection sampling).
/// Draws an index in `0..n` without bias (rejection sampling).
struct UniformPicker {
    buf: Zeroizing<[u8; 64]>,
    pos: usize,
}

impl UniformPicker {
    fn new() -> Self {
        Self { buf: Zeroizing::new([0; 64]), pos: 64 }
    }

    fn next_byte(&mut self) -> Result<u8> {
        if self.pos == self.buf.len() {
            fill_random(self.buf.as_mut())?;
            self.pos = 0;
        }
        self.pos += 1;
        Ok(self.buf[self.pos - 1])
    }

    fn index(&mut self, n: usize) -> Result<usize> {
        debug_assert!((1..=256).contains(&n));
        // Maior múltiplo de n que cabe em 256; bytes acima dele são descartados.
        // Largest multiple of n that fits in 256; bytes above it are discarded.
        let limit = 256 - (256 % n);
        loop {
            let b = self.next_byte()? as usize;
            if b < limit {
                return Ok(b % n);
            }
        }
    }
}

/// Gera uma senha. Se faltar algum tipo escolhido, sorteia a senha inteira de novo.
/// Generates a password. If a chosen type is missing, the whole password is redrawn.
pub fn generate(options: &GeneratorOptions) -> Result<Zeroizing<String>> {
    let classes = options.validate()?;
    let alphabet: Vec<char> = classes.iter().flatten().copied().collect();
    let mut picker = UniformPicker::new();
    loop {
        let mut out = Zeroizing::new(String::with_capacity(options.length));
        for _ in 0..options.length {
            out.push(alphabet[picker.index(alphabet.len())?]);
        }
        if classes.iter().all(|set| out.chars().any(|c| set.contains(&c))) {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_all_types_and_length() {
        let opts = GeneratorOptions::default();
        for _ in 0..200 {
            let p = generate(&opts).unwrap();
            assert_eq!(p.chars().count(), 24);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
        }
    }

    #[test]
    fn excludes_ambiguous() {
        let opts = GeneratorOptions { length: 128, exclude_ambiguous: true, ..Default::default() };
        for _ in 0..50 {
            assert!(!generate(&opts).unwrap().chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn respects_disabled_types() {
        let opts = GeneratorOptions { length: 64, symbols: false, uppercase: false, ..Default::default() };
        let p = generate(&opts).unwrap();
        assert!(p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn rejects_invalid_options() {
        let none = GeneratorOptions { lowercase: false, uppercase: false, digits: false, symbols: false, ..Default::default() };
        assert!(generate(&none).is_err());
        assert!(generate(&GeneratorOptions { length: 0, ..Default::default() }).is_err());
        assert!(generate(&GeneratorOptions { length: 129, ..Default::default() }).is_err());
        assert!(generate(&GeneratorOptions { length: 3, ..Default::default() }).is_err());
        assert!(generate(&GeneratorOptions { length: 128, ..Default::default() }).is_ok());
    }

    #[test]
    fn entropy_matches_plan() {
        // 20 caracteres com todos os tipos ≈ 130 bits (planejamento).
        // 20 characters with every type ≈ 130 bits (plan).
        let e = GeneratorOptions { length: 20, ..Default::default() }.entropy_bits().unwrap();
        assert!((129.0..131.2).contains(&e), "{e}");
        // Só dígitos: exatamente L·log2(10). / Digits only: exactly L·log2(10).
        let d = GeneratorOptions { length: 10, lowercase: false, uppercase: false, symbols: false, ..Default::default() };
        assert!((d.entropy_bits().unwrap() - 10.0 * 10f64.log2()).abs() < 1e-9);
        // Excluir ambíguos reduz a entropia. / Excluding ambiguous chars lowers entropy.
        let a = GeneratorOptions { length: 20, exclude_ambiguous: true, ..Default::default() };
        assert!(a.entropy_bits().unwrap() < e);
    }

    #[test]
    fn distribution_is_roughly_uniform() {
        // Teste estatístico simples: cada dígito deve sair perto de 10% das vezes.
        // Simple statistical test: each digit should appear close to 10% of the time.
        let opts = GeneratorOptions { length: 100, lowercase: false, uppercase: false, symbols: false, ..Default::default() };
        let mut counts = [0usize; 10];
        for _ in 0..200 {
            for c in generate(&opts).unwrap().chars() {
                counts[c.to_digit(10).unwrap() as usize] += 1;
            }
        }
        for c in counts {
            assert!((1700..2300).contains(&c), "{counts:?}");
        }
    }
}
