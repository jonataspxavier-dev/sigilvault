# SigilVault — App desktop / Desktop app

## Português

Esta pasta vai receber o app desktop do SigilVault na **Fase 2**:

- **Tauri 2** com interface em TypeScript, usando o WebView2 que já vem no Windows.
- Telas de criação e desbloqueio do cofre; listar, buscar, criar, editar e excluir itens.
- Gerador de senhas na interface.
- **Serviço da bandeja** sempre ativo e leve (~10–20 MB), com bloqueio automático.
- A janela só é carregada quando aberta e é destruída ao bloquear.

Toda a criptografia fica no [`core/`](../core/). O app apenas chama o núcleo.

## English

This folder will hold the SigilVault desktop app in **Phase 2**:

- **Tauri 2** with a TypeScript UI, using the WebView2 that ships with Windows.
- Vault creation and unlock screens; list, search, create, edit and delete items.
- Password generator in the UI.
- An always-on, lightweight **tray service** (~10–20 MB) with auto-lock.
- The window loads only when opened and is destroyed on lock.

All cryptography lives in [`core/`](../core/). The app only calls the core.
