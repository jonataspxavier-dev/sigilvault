# SigilVault — App desktop / Desktop app

## Português

O app desktop do SigilVault (Fases 2, 3 e 6, prontas):

- **Tauri 2** com interface em TypeScript puro, usando o WebView2 que já vem no Windows.
- Criação, desbloqueio e recuperação do cofre; listar, buscar, criar, editar e excluir itens, com histórico de senhas.
- Gerador de senhas ("Forja") e relatório de saúde das senhas ("Vigília").
- Exportar e importar o cofre, backups locais e importação de outros gerenciadores.
- **Serviço da bandeja** sempre ativo e leve (~10–20 MB), com bloqueio automático. A janela só é carregada quando aberta e é destruída ao bloquear.
- Ponte com a extensão do navegador (pipe protegido e pareamento OPAQUE).
- Atalhos globais, busca rápida, Auto-Type e o selo nos campos de senha de outros programas.

Organização:

- `src/`: interface em TypeScript (`views/`, `i18n.ts`, estilos).
- `src-tauri/`: parte em Rust (comandos, estado, bandeja, `bridge/` para a extensão, `win/` para as funções do Windows e `installer/` para o instalador NSIS).

Toda a criptografia fica no [`core/`](../core/). O app apenas chama o núcleo.

## English

The SigilVault desktop app (Phases 2, 3 and 6, done):

- **Tauri 2** with a plain TypeScript UI, using the WebView2 that ships with Windows.
- Vault creation, unlock and recovery; list, search, create, edit and delete items, with password history.
- Password generator ("Forge") and password health report ("Watch").
- Vault export and import, local backups and import from other managers.
- An always-on, lightweight **tray service** (~10–20 MB) with auto-lock. The window loads only when opened and is destroyed on lock.
- Bridge with the browser extension (protected pipe and OPAQUE pairing).
- Global hotkeys, quick search, Auto-Type and the seal on other programs' password fields.

Layout:

- `src/`: TypeScript UI (`views/`, `i18n.ts`, styles).
- `src-tauri/`: the Rust side (commands, state, tray, `bridge/` for the extension, `win/` for Windows features and `installer/` for the NSIS installer).

All cryptography lives in [`core/`](../core/). The app only calls the core.
