# SigilVault

🇧🇷 [Português](#português) · 🇺🇸 [English](#english)

---

## Português

**SigilVault** é um gerenciador de senhas **local** para Windows. Ele guarda tudo criptografado no próprio computador, preenche senhas no navegador e em apps desktop e gera senhas fortes. Nada vai para a nuvem, a menos que você escolha.

> Status: **Fase 2 em andamento**: o núcleo em Rust está pronto e o app desktop já cria, desbloqueia, recupera e edita o cofre.

### Visão geral

- **Segurança em primeiro lugar:** os dados só são descriptografados na memória, na hora de usar.
- **Local-first:** sem servidor, sem conta online e sem telemetria.
- **Leve:** um serviço na bandeja do Windows de ~10–20 MB; a janela só é carregada quando aberta.
- **Senhas fortes como hábito:** toda senha nova nasce do gerador.
- **Identidade própria:** nome, ícones e visual 100% originais, com estilo medieval nórdico.

### Arquitetura

```
Extensão do navegador ──Native Messaging (pareado)──► Serviço da bandeja ──► Núcleo Rust ──► cofre.sigil
Apps desktop ◄──────────── Auto-Type (atalho) ─────── Janela Tauri (só quando aberta)
```

| Pasta | Conteúdo |
| --- | --- |
| [`core/`](core/) | Núcleo em Rust: cofre, criptografia, slots de chave, códigos de recuperação e gerador. É o **único** componente que descriptografa. |
| [`app/`](app/) | App desktop em Tauri 2 + TypeScript (Fase 2). |
| [`extension/`](extension/) | Extensão WebExtensions MV3 para Chrome, Brave, Opera e Firefox (Fase 4). |
| [`docs/`](docs/) | [Planejamento completo](docs/PLANEJAMENTO.md). |

### Modelo de segurança

- **Senha mestra → chave:** Argon2id (padrão de 128 MiB e 3 passadas; mínimos obrigatórios ao abrir o cofre).
- **Chave do cofre:** 256 bits aleatórios, guardados criptografados em *slots*: um pela senha mestra e um por código de recuperação.
- **Dados:** XChaCha20-Poly1305 (criptografia autenticada). O cabeçalho inteiro é autenticado, e cada senha fica amarrada ao ID do seu item (AAD).
- **10 códigos de recuperação** de uso único, com 130 bits cada (base32 + dígito verificador). Usar um código troca a chave do cofre e recriptografa tudo.
- **Aleatoriedade:** sempre o gerador seguro do sistema operacional.
- **Bibliotecas:** apenas as auditadas do ecossistema RustCrypto. Nada de criptografia própria.
- **Protege contra:** roubo do PC ou do disco, vazamento do arquivo ou do backup, phishing e vazamento de nuvem.
- **Não protege contra:** malware ou keylogger rodando enquanto o cofre está desbloqueado.

### Fases

1. Núcleo em Rust + testes ✅
2. **App desktop** ← *atual* (Tauri): criar e desbloquear o cofre, CRUD, gerador, bandeja e bloqueio automático
3. Exportar e importar, backups locais e importação de outros gerenciadores
4. Extensão + Native Messaging com pareamento seguro
5. Navegador completo: preencher, balão "gerar ou só salvar" e aviso de salvar
6. Desktop completo: atalhos globais, pop-up de busca e Auto-Type
7. Endurecimento e lançamento: revisão de segurança, instalador assinado e lojas

### Desenvolvimento

```powershell
cargo test --workspace          # testes do núcleo e do app (Rust)
cd app; npm install; npm test   # testes da interface
npx tauri dev                   # abre o app em modo de desenvolvimento
```

Requisitos: Rust estável (rustup), Node.js LTS e Visual Studio Build Tools com C++.

---

## English

**SigilVault** is a **local** password manager for Windows. It keeps everything encrypted on your own computer, fills passwords in the browser and in desktop apps, and generates strong passwords. Nothing goes to the cloud unless you choose so.

> Status: **Phase 2 in progress**: the Rust core is done and the desktop app already creates, unlocks, recovers and edits the vault.

### Overview

- **Security first:** data is decrypted only in memory, only when needed.
- **Local-first:** no server, no online account, no telemetry.
- **Lightweight:** a ~10–20 MB Windows tray service; the window loads only when opened.
- **Strong passwords by habit:** every new password comes from the generator.
- **Own identity:** 100% original name, icons and look, in a Nordic medieval style.

### Architecture

```
Browser extension ──Native Messaging (paired)──► Tray service ──► Rust core ──► cofre.sigil
Desktop apps ◄──────────── Auto-Type (hotkey) ── Tauri window (only when open)
```

| Folder | Contents |
| --- | --- |
| [`core/`](core/) | Rust core: vault, cryptography, key slots, recovery codes and generator. It is the **only** component that decrypts. |
| [`app/`](app/) | Tauri 2 + TypeScript desktop app (Phase 2). |
| [`extension/`](extension/) | MV3 WebExtension for Chrome, Brave, Opera and Firefox (Phase 4). |
| [`docs/`](docs/) | [Full plan (Portuguese)](docs/PLANEJAMENTO.md). |

### Security model

- **Master password → key:** Argon2id (default 128 MiB and 3 passes; minimums enforced when opening a vault).
- **Vault key:** 256 random bits, stored encrypted in *slots*: one for the master password and one per recovery code.
- **Data:** XChaCha20-Poly1305 (authenticated encryption). The whole header is authenticated, and each password is bound to its item ID (AAD).
- **10 single-use recovery codes**, 130 bits each (base32 + check digit). Using a code rotates the vault key and re-encrypts everything.
- **Randomness:** always the operating system's secure RNG.
- **Libraries:** only audited RustCrypto crates. No home-made cryptography.
- **Protects against:** stolen PC or disk, leaked file or backup, phishing and cloud leaks.
- **Does not protect against:** malware or keyloggers running while the vault is unlocked.

### Phases

1. Rust core + tests ✅
2. **Desktop app** ← *current* (Tauri): create and unlock the vault, CRUD, generator, tray and auto-lock
3. Export and import, local backups and import from other managers
4. Extension + Native Messaging with secure pairing
5. Full browser support: fill, "generate or just save" popup and save prompt
6. Full desktop support: global hotkeys, search popup and Auto-Type
7. Hardening and release: security review, signed installer and store publishing

### Development

```powershell
cargo test --workspace          # core and app tests (Rust)
cd app; npm install; npm test   # UI tests
npx tauri dev                   # opens the app in development mode
```

Requirements: stable Rust (rustup), Node.js LTS and Visual Studio Build Tools with C++.
