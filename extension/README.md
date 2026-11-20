# SigilVault — Extensão do navegador / Browser extension

## Português

Extensão WebExtensions **Manifest V3** em TypeScript. Um pacote serve ao Chrome, Brave e Opera (`dist/chromium`), e outro, com o manifesto ajustado, serve ao Firefox (`dist/firefox`).

**Fase 4 (pronta): ligação segura com o app.**
- Conversa com o app pelo **Native Messaging** (`sigilvault-host.exe`), sem abrir porta de rede.
- **Pareamento OPAQUE**: o app mostra um código de 8 caracteres, você digita na extensão, e o código vale por 2 minutos e 3 tentativas. A parte criptográfica é o mesmo código Rust do app (`crates/pairing`), compilado para WebAssembly.
- A chave de pareamento fica no IndexedDB como `CryptoKey` **não extraível**: nem a própria extensão consegue ler os bytes.
- Cada conexão abre um canal **AES-256-GCM** com chaves novas e contador contra repetição.
- O ícone da barra mostra o cofre **trancado**, **aberto** ou **sem conexão**, e muda na hora.
- **Não guarda senhas** e, nesta fase, **não acessa nenhuma página**: a única permissão é `nativeMessaging`.

**Fase 5 (próxima):**
- preencher logins com um clique, comparando a origem completa;
- balão "gerar senha forte ou só salvar";
- aviso de salvar depois de enviar um formulário.

### Como testar (desenvolvimento)

```powershell
# 1. Compilar o app e o host (na raiz do repositório)
cargo build -p sigilvault-host -p sigilvault-app
# 2. Registrar o host para o seu usuário (HKCU). Para desfazer: -Unregister
.\scripts\register-host.ps1
# 3. Compilar a extensão
cd extension; npm install; npm run build
# 4. Abrir o app
cd ..\app; npx tauri dev
```

Depois, carregue a extensão no navegador:
- **Chrome / Brave / Opera:** abra `chrome://extensions` (no Opera, `opera://extensions`), ligue o **Modo do desenvolvedor**, clique em **Carregar sem compactação** e escolha `extension/dist/chromium`. O ID precisa ser `jclandjcanimgllfngognekhhhnomimf`.
- **Firefox:** abra `about:debugging#/runtime/this-firefox`, clique em **Carregar extensão temporária** e escolha `extension/dist/firefox/manifest.json`.

Para parear, clique no ícone do SigilVault, depois em **Parear com o app**, e digite o código que aparece na janela do app.

### Testes

- `npm test`: testes de unidade. Inclui o vetor fixo que prova que o canal em WebCrypto é idêntico ao do Rust.
- `SIGILVAULT_E2E=1 npm test`: ponta a ponta com o app real, fazendo o papel do navegador. Veja os pré-requisitos no topo de `src/host.e2e.test.ts`. **Nunca rode contra o seu cofre real.**

## English

**Manifest V3** WebExtensions extension in TypeScript. One package serves Chrome, Brave and Opera (`dist/chromium`), and another, with an adjusted manifest, serves Firefox (`dist/firefox`).

**Phase 4 (done): secure link with the app.**
- Talks to the app over **Native Messaging** (`sigilvault-host.exe`), with no network port.
- **OPAQUE pairing**: the app shows an 8-character code, you type it in the extension, and the code is valid for 2 minutes and 3 attempts. The cryptographic part is the same Rust code as the app (`crates/pairing`), compiled to WebAssembly.
- The pairing key lives in IndexedDB as a **non-extractable** `CryptoKey`: not even the extension can read its bytes.
- Each connection opens an **AES-256-GCM** channel with fresh keys and a replay counter.
- The toolbar icon shows the vault as **locked**, **unlocked** or **not connected**, and changes instantly.
- **Stores no passwords** and, in this phase, **accesses no web page**: the only permission is `nativeMessaging`.

**Phase 5 (next):**
- one-click login fill, matching the full origin;
- "generate a strong password or just save" popup;
- offer to save after a form is submitted.

### How to test (development)

```powershell
# 1. Build the app and the host (at the repository root)
cargo build -p sigilvault-host -p sigilvault-app
# 2. Register the host for your user (HKCU). To undo: -Unregister
.\scripts\register-host.ps1
# 3. Build the extension
cd extension; npm install; npm run build
# 4. Start the app
cd ..\app; npx tauri dev
```

Then load the extension in the browser:
- **Chrome / Brave / Opera:** open `chrome://extensions` (in Opera, `opera://extensions`), turn on **Developer mode**, click **Load unpacked** and pick `extension/dist/chromium`. The ID must be `jclandjcanimgllfngognekhhhnomimf`.
- **Firefox:** open `about:debugging#/runtime/this-firefox`, click **Load Temporary Add-on** and pick `extension/dist/firefox/manifest.json`.

To pair, click the SigilVault icon, then **Pair with the app**, and type the code shown in the app window.

### Tests

- `npm test`: unit tests. Includes the fixed vector that proves the WebCrypto channel is identical to the Rust one.
- `SIGILVAULT_E2E=1 npm test`: end-to-end with the real app, playing the browser. See the prerequisites at the top of `src/host.e2e.test.ts`. **Never run it against your real vault.**
