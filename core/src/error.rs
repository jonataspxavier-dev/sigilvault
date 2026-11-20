//! Tipos de erro do núcleo.
//! Core error types.

use thiserror::Error;

/// Resultado padrão do núcleo.
/// Standard core result.
pub type Result<T> = std::result::Result<T, Error>;

/// Erros possíveis do núcleo. As mensagens não revelam segredos.
/// Possible core errors. Messages never reveal secrets.
#[derive(Debug, Error)]
pub enum Error {
    /// Senha mestra ou código de recuperação incorreto. É o mesmo erro para os dois,
    /// para não dar pistas a quem tenta adivinhar.
    /// Wrong master password or recovery code. Same error for both, so guessers learn nothing.
    #[error("credenciais inválidas / invalid credentials")]
    InvalidCredentials,

    /// O código digitado está malformado (tamanho, caracteres ou dígito verificador).
    /// The typed code is malformed (length, characters or check digit).
    #[error("código de recuperação malformado / malformed recovery code")]
    MalformedRecoveryCode,

    /// O arquivo foi alterado ou corrompido: a autenticação falhou.
    /// The file was altered or corrupted: authentication failed.
    #[error("arquivo do cofre adulterado ou corrompido / vault file tampered or corrupted")]
    Tampered,

    /// O arquivo não segue o formato esperado.
    /// The file does not follow the expected format.
    #[error("formato de arquivo inválido: {0} / invalid file format: {0}")]
    InvalidFormat(&'static str),

    /// Versão do formato não suportada por este programa.
    /// File format version not supported by this program.
    #[error("versão de formato não suportada: {0} / unsupported format version: {0}")]
    UnsupportedVersion(u16),

    /// Parâmetros do Argon2 abaixo do mínimo ou acima do máximo permitido.
    /// Argon2 parameters below the minimum or above the allowed maximum.
    #[error("parâmetros de derivação de chave inválidos / invalid key derivation parameters")]
    WeakKdfParams,

    /// A senha mestra é fraca demais.
    /// The master password is too weak.
    #[error("senha mestra fraca demais / master password too weak")]
    WeakMasterPassword,

    /// Opções inválidas do gerador de senhas.
    /// Invalid password generator options.
    #[error("opções do gerador inválidas: {0} / invalid generator options: {0}")]
    InvalidGeneratorOptions(&'static str),

    /// Item não encontrado no cofre.
    /// Item not found in the vault.
    #[error("item não encontrado / item not found")]
    ItemNotFound,

    /// Dado de entrada inválido (ex.: nome de usuário vazio).
    /// Invalid input (e.g. empty username).
    #[error("entrada inválida: {0} / invalid input: {0}")]
    InvalidInput(&'static str),

    /// Não há mais códigos de recuperação.
    /// No recovery codes left.
    #[error("não restam códigos de recuperação / no recovery codes left")]
    NoRecoveryCodesLeft,

    /// Arquivo de importação em formato desconhecido ou não suportado (ex.: exportação cifrada).
    /// Import file in an unknown or unsupported format (e.g. encrypted export).
    #[error("formato de importação não suportado: {0} / unsupported import format: {0}")]
    UnsupportedImport(&'static str),

    /// Falha do gerador aleatório do sistema operacional.
    /// Operating system random generator failure.
    #[error("falha no gerador aleatório do sistema / system random generator failure")]
    Rng,

    /// Erro interno de criptografia (não deveria acontecer).
    /// Internal cryptography error (should not happen).
    #[error("erro interno de criptografia / internal cryptography error")]
    Crypto,

    /// Erro de leitura ou gravação em disco.
    /// Disk read or write error.
    #[error("erro de E/S: {0} / I/O error: {0}")]
    Io(#[from] std::io::Error),
}
