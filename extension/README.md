# SigilVault — Extensão do navegador / Browser extension

## Português

Extensão WebExtensions **Manifest V3** em TypeScript. Um pacote serve ao Chrome, Brave e Opera (`dist/chromium`), e outro, com o manifesto ajustado, serve ao Firefox (`dist/firefox`).

**Fase 4 (pronta): ligação segura com o app.**
- Conversa com o app pelo **Native Messaging** (`sigilvault-host.exe`), sem abrir porta de rede.
- **Pareamento OPAQUE**: o app mostra um código de 8 caracteres, você digita na extensão, e o código vale por 2 minutos e 3 tentativas. A parte criptográfica é o mesmo código Rust do app (`crates/pairing`), compilado para WebAssembly.
- A chave de pareamento fica no IndexedDB como `CryptoKey` **não extraível**: nem a própria extensão consegue ler os bytes.
- Cada conexão abre um canal **AES-256-GCM** com chaves novas e contador contra repetição.
- O ícone da barra mostra o cofre **trancado**, **aberto** ou **sem conexão**, e muda na hora.
- **Não guarda senhas.**

**Fase 5 (pronta): navegador completo.**
- Ícone do SigilVault (um selo de cera) dentro dos campos de login. Um clique abre o balão com as contas **daquele site**; outro clique preenche. Nunca preenche sozinho.
- Em cadastros e trocas de senha: **Gerar senha forte** (tamanho, tipos de caractere, força em bits, "Gerar outra", "Usar e salvar") ou **Só salvar** a senha que você digitou. As opções preferidas viram padrão.
- Depois de enviar um formulário: **"Salvar essa senha?"** ou **"Atualizar a senha?"**, só se o login for novo ou tiver mudado. A senha antiga vai para o histórico.
- Proteções:
  - o ícone e o balão são uma página da extensão dentro de um iframe, numa *shadow root* fechada; a página não lê nada e não consegue clicar lá dentro;
  - a senha salva vai do app direto ao campo, nunca passa pelo balão;
  - a origem vem do navegador, e o app confere a regra de origem de novo;
  - o primeiro preenchimento em cada site pede confirmação no app; formulários dentro de outro site perguntam sempre;
  - cliques só valem se forem reais e 500 ms depois de o conteúdo aparecer, e o Chrome, o Brave e o Opera também conferem se o balão está mesmo visível;
  - se a página mexer no elemento do balão, ele fecha.
- Permissões: Native Messaging e páginas `http`/`https`. Nunca arquivos locais.

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
- **Stores no passwords.**

**Phase 5 (done): full browser support.**
- SigilVault icon (a wax seal) inside login fields. One click opens the balloon with the accounts **for that site**; another click fills. It never fills by itself.
- On sign-up and password-change forms: **Generate a strong password** (length, character types, strength in bits, "Generate another", "Use and save") or **Just save** the password you typed. Preferred options become the default.
- After a form is submitted: **"Save this password?"** or **"Update the password?"**, only if the login is new or changed. The old password goes to the history.
- Protections:
  - the icon and balloon are an extension page inside an iframe, in a closed shadow root; the page cannot read anything or click inside;
  - the saved password goes from the app straight to the field, never through the balloon;
  - the origin comes from the browser, and the app checks the origin rule again;
  - the first fill on each site asks for confirmation in the app; forms inside another site always ask;
  - clicks only count if real and 500 ms after the content appeared, and Chrome, Brave and Opera also check that the balloon is truly visible;
  - if the page touches the balloon element, it closes.
- Permissions: Native Messaging and `http`/`https` pages. Never local files.

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
