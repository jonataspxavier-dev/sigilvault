# Instalar o SigilVault / Installing SigilVault

🇧🇷 [Português](#português) · 🇺🇸 [English](#english)

---

## Português

Versão de testes. Ainda não tem certificado de assinatura nem está nas lojas de extensões, então a instalação tem alguns passos manuais.

### 1. Instalar o app

1. Baixe `SigilVault_<versão>_x64-setup.exe` em [Releases](https://github.com/jonataspxavier-dev/sigilvault/releases).
2. Confira o arquivo no PowerShell: `Get-FileHash .\SigilVault_<versão>_x64-setup.exe`. O resultado tem que ser igual ao SHA-256 escrito na página da versão.
3. Rode o instalador. O Windows vai mostrar **"O Windows protegeu o computador"** (SmartScreen), porque o instalador não é assinado. Clique em **Mais informações** → **Executar assim mesmo**. Só faça isso com o arquivo baixado daqui e conferido.
4. Aceite o pedido de administrador. O app é instalado em `C:\Program Files\SigilVault`. Essa pasta só pode ser alterada por um administrador, e o app confia no conector da extensão justamente por isso. **Não copie os arquivos para outra pasta:** fora dela a extensão não conecta.
5. Abra o SigilVault e crie o cofre. **Guarde os 10 códigos de recuperação** em papel. Sem a senha mestra e sem os códigos, ninguém (nem quem fez o app) consegue abrir o cofre.

### 2. Instalar a extensão

**Chrome, Brave ou Opera**

1. Abra `chrome://extensions` (no Brave, `brave://extensions`; no Opera, `opera://extensions`).
2. Ligue o **Modo do desenvolvedor**.
3. Clique em **Carregar sem compactação** e escolha `C:\Program Files\SigilVault\extension\chromium`.
4. Fixe o ícone da SigilVault na barra, clique nele e siga o pareamento: o app mostra um código de 8 caracteres e você o digita na extensão.

**Firefox** (por enquanto, só temporária)

1. Abra `about:debugging#/runtime/this-firefox`.
2. Clique em **Carregar extensão temporária** e escolha `C:\Program Files\SigilVault\extension\firefox\manifest.json`.
3. Ela some ao fechar o Firefox. A versão permanente depende de assinatura da Mozilla, que vem depois.

### 3. Uso no dia a dia

- **Bloqueio:** em Configurações, escolha "Bloquear após X minutos" ou "Manter aberto". Nos dois casos o cofre bloqueia com Win+L, ao suspender ou ao sair da conta.
- **Programas do computador:** ao clicar no campo de senha de um programa, o dragão aparece dentro do campo. Clique nele para preencher ou salvar uma conta. Depois de entrar digitando à mão, o SigilVault oferece guardar o login. Ele nunca lê o que você digita.
- **Configurações → Iniciar com o Windows** deixa o app na bandeja ao ligar o PC. O cofre continua pedindo a senha.
- **Saúde** mostra senhas fracas, repetidas ou antigas.
- Atalhos: `Ctrl+Shift+Espaço` abre a busca e digita em programas; `Ctrl+Shift+G` gera uma senha nova.
- Recomendado: ative o **BitLocker** (criptografia do disco do Windows).

### 4. Atualizar e desinstalar

- **Atualizar:** rode o instalador novo por cima. Não há atualização automática.
- **Seus dados** ficam em `%APPDATA%\app.sigilvault.desktop` (cofre e backups). Reinstalar ou desinstalar **não apaga** o cofre.
- **Desinstalar:** Configurações do Windows → Aplicativos → SigilVault. Depois remova a extensão do navegador.

---

## English

A test build. It has no signing certificate yet and is not in the extension stores, so installing takes a few manual steps.

### 1. Install the app

1. Download `SigilVault_<version>_x64-setup.exe` from [Releases](https://github.com/jonataspxavier-dev/sigilvault/releases).
2. Verify the file in PowerShell: `Get-FileHash .\SigilVault_<version>_x64-setup.exe`. The result must match the SHA-256 on the release page.
3. Run the installer. Windows will show **"Windows protected your PC"** (SmartScreen), because the installer is not signed. Click **More info** → **Run anyway**. Only do this with a file downloaded from here and verified.
4. Accept the administrator prompt. The app is installed to `C:\Program Files\SigilVault`. Only an administrator can change that folder, which is exactly why the app trusts the extension connector. **Do not copy the files elsewhere:** outside that folder the extension will not connect.
5. Open SigilVault and create your vault. **Keep the 10 recovery codes** on paper. Without the master password and the codes, nobody (not even the app's author) can open the vault.

### 2. Install the extension

**Chrome, Brave or Opera**

1. Open `chrome://extensions` (Brave: `brave://extensions`; Opera: `opera://extensions`).
2. Turn on **Developer mode**.
3. Click **Load unpacked** and pick `C:\Program Files\SigilVault\extension\chromium`.
4. Pin the SigilVault icon, click it and follow the pairing: the app shows an 8-character code and you type it into the extension.

**Firefox** (temporary for now)

1. Open `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on** and pick `C:\Program Files\SigilVault\extension\firefox\manifest.json`.
3. It goes away when Firefox closes. A permanent version needs Mozilla's signature, which comes later.

### 3. Everyday use

- **Locking:** in Settings, pick "Lock after X minutes" or "Keep open". Either way the vault locks on Win+L, on sleep or on sign-out.
- **Desktop programs:** when you click a program's password field, the dragon shows up inside it. Click it to fill or save an account. After you sign in by typing, SigilVault offers to keep the login. It never reads what you type.
- **Settings → Start with Windows** keeps the app in the tray at sign-in. The vault still asks for the password.
- **Health** lists weak, reused or old passwords.
- Hotkeys: `Ctrl+Shift+Space` opens search and types into programs; `Ctrl+Shift+G` generates a new password.
- Recommended: turn on **BitLocker** (Windows disk encryption).

### 4. Update and uninstall

- **Update:** run the new installer over the old one. There are no automatic updates.
- **Your data** lives in `%APPDATA%\app.sigilvault.desktop` (vault and backups). Reinstalling or uninstalling does **not** delete the vault.
- **Uninstall:** Windows Settings → Apps → SigilVault. Then remove the extension from your browser.
