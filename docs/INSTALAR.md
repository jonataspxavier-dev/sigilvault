# Instalar o SigilVault / Installing SigilVault

🇧🇷 [Português](#português) · 🇺🇸 [English](#english) · 🐧 Linux: [PT](#linux-versão-de-testes) · [EN](#linux-test-build)

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

## Linux (versão de testes)

Funciona em distribuições com GTK 3 e WebKitGTK 4.1 (Ubuntu 22.04 ou mais novo, Debian 12, Fedora 38 e parecidas), em Wayland e em X11. O que o Linux ainda não tem está no fim desta seção.

### 1. Instalar o app

1. Baixe o pacote em [Releases](https://github.com/jonataspxavier-dev/sigilvault/releases): `.deb` (Debian, Ubuntu e derivadas) ou `.rpm` (Fedora e derivadas).
2. Confira o SHA-256 (`sha256sum arquivo`) com o escrito na página da versão.
3. Instale: `sudo apt install ./SigilVault_<versão>_amd64.deb` ou `sudo dnf install ./SigilVault-<versão>.x86_64.rpm`.
4. Abra o SigilVault e crie o cofre. **Guarde os 10 códigos de recuperação** em papel.

O app fica em `/usr/bin`, pasta que só o administrador altera. É por isso que o app confia no conector da extensão, então **não copie os arquivos para outra pasta**.

### 2. Instalar a extensão

A extensão fica em `/usr/lib/SigilVault/extension/`. O pacote já registra o conector para Chrome, Chromium, Edge e Firefox; ao abrir, o app registra também Brave, Vivaldi e Opera que estiverem instalados.

- **Chrome, Chromium, Brave, Edge, Vivaldi ou Opera:** abra a página de extensões, ligue o **Modo do desenvolvedor**, clique em **Carregar sem compactação** e escolha `/usr/lib/SigilVault/extension/chromium`.
- **Firefox:** abra `about:debugging#/runtime/this-firefox`, clique em **Carregar extensão temporária** e escolha `/usr/lib/SigilVault/extension/firefox/manifest.json`.

Depois, fixe o ícone, clique nele e siga o pareamento: o app mostra um código de 8 caracteres e você o digita na extensão.

> **Navegadores em snap** (Brave, Chromium e Firefox do Ubuntu) rodam isolados: não leem os manifestos do sistema nem executam o conector, e a extensão mostra "O conector do SigilVault não foi encontrado". Use o navegador em `.deb`: Google Chrome (baixe o `.deb` em google.com/chrome), Brave (repositório oficial em brave.com/linux) ou Firefox da Mozilla (repositório `packages.mozilla.org`).

### 3. Uso no dia a dia

- **Atalhos:** no Wayland, o atalho passa pelo seu desktop, que pede permissão na primeira vez (GNOME 48 ou mais novo, KDE). Se o seu desktop não oferecer isso, crie um atalho do sistema que rode `sigilvault-app --quick search` (busca) e `sigilvault-app --quick generate` (gerar senha).
- **Programas do computador:** ao clicar num campo de senha de outro programa, aparece uma notificação "Abrir o SigilVault aqui". O texto é escrito no campo pela acessibilidade do sistema, então o programa precisa expor o campo (programas GTK e Qt expõem). Se não der, use **Copiar**.
- **Bloqueio:** o cofre bloqueia por inatividade, ao travar a tela, ao suspender e ao sair da conta.
- **Chaveiro:** a chave que protege os arquivos de apoio (pareamentos, contador de versão) fica no chaveiro do sistema (GNOME Keyring, KWallet). Sem chaveiro, ela fica num arquivo só seu na pasta de dados, o que protege menos.
- **Bandeja no GNOME:** o GNOME não mostra ícones de bandeja sozinho. Instale a extensão **AppIndicator** (`gnome-shell-extension-appindicator`). Sem ela o app funciona, mas sem o ícone.
- **Seus dados** ficam em `~/.local/share/app.sigilvault.desktop` (cofre e backups), com acesso só seu. Desinstalar o pacote **não apaga** o cofre.
- Recomendado: criptografe o disco (LUKS).

### O que o Linux ainda não tem

- O aviso "Guardar este login?" depois de entrar num programa.
- O selo desenhado dentro do campo (no Linux é uma notificação).
- Digitar teclas em programas que não expõem o campo pela acessibilidade.
- AppImage com a extensão do navegador (o AppImage abre o cofre e os programas, mas a ponte com o navegador exige o `.deb` ou o `.rpm`) e Flatpak.

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

---

## Linux (test build)

Works on distributions with GTK 3 and WebKitGTK 4.1 (Ubuntu 22.04 or newer, Debian 12, Fedora 38 and similar), on Wayland and X11. What Linux does not have yet is at the end of this section.

### 1. Install the app

1. Download the package from [Releases](https://github.com/jonataspxavier-dev/sigilvault/releases): `.deb` (Debian, Ubuntu and derivatives) or `.rpm` (Fedora and derivatives).
2. Check the SHA-256 (`sha256sum file`) against the one on the release page.
3. Install: `sudo apt install ./SigilVault_<version>_amd64.deb` or `sudo dnf install ./SigilVault-<version>.x86_64.rpm`.
4. Open SigilVault and create your vault. **Keep the 10 recovery codes** on paper.

The app lives in `/usr/bin`, a folder only an administrator can change. That is why the app trusts the extension connector, so **do not copy the files to another folder**.

### 2. Install the extension

The extension lives in `/usr/lib/SigilVault/extension/`. The package already registers the connector for Chrome, Chromium, Edge and Firefox; when it opens, the app also registers any installed Brave, Vivaldi and Opera.

- **Chrome, Chromium, Brave, Edge, Vivaldi or Opera:** open the extensions page, turn on **Developer mode**, click **Load unpacked** and pick `/usr/lib/SigilVault/extension/chromium`.
- **Firefox:** open `about:debugging#/runtime/this-firefox`, click **Load Temporary Add-on** and pick `/usr/lib/SigilVault/extension/firefox/manifest.json`.

Then pin the icon, click it and follow the pairing: the app shows an 8-character code and you type it into the extension.

> **Browsers as snaps** (Ubuntu's Brave, Chromium and Firefox) run isolated: they do not read the system manifests nor run the connector, and the extension shows "SigilVault connector not found". Use the browser as a `.deb`: Google Chrome (download the `.deb` from google.com/chrome), Brave (official repository at brave.com/linux) or Mozilla's Firefox (`packages.mozilla.org` repository).

### 3. Everyday use

- **Hotkeys:** on Wayland the hotkey goes through your desktop, which asks for permission the first time (GNOME 48 or newer, KDE). If your desktop does not offer that, create a system shortcut that runs `sigilvault-app --quick search` (search) and `sigilvault-app --quick generate` (generate a password).
- **Desktop programs:** when you click another program's password field, a notification "Open SigilVault here" appears. The text is written into the field through the system's accessibility, so the program must expose the field (GTK and Qt programs do). If it cannot, use **Copy**.
- **Locking:** the vault locks on inactivity, when the screen locks, on sleep and on sign-out.
- **Keyring:** the key protecting the support files (pairings, version counter) lives in the system keyring (GNOME Keyring, KWallet). Without a keyring, it lives in a file only you can read in the data folder, which protects less.
- **Tray on GNOME:** GNOME does not show tray icons by itself. Install the **AppIndicator** extension (`gnome-shell-extension-appindicator`). Without it the app works, just without the icon.
- **Your data** lives in `~/.local/share/app.sigilvault.desktop` (vault and backups), readable only by you. Uninstalling the package does **not** delete the vault.
- Recommended: encrypt your disk (LUKS).

### What Linux does not have yet

- The "Keep this login?" notice after signing in to a program.
- The seal drawn inside the field (on Linux it is a notification).
- Typing keys into programs that do not expose the field through accessibility.
- AppImage with the browser extension (the AppImage opens the vault and programs, but the browser bridge needs the `.deb` or `.rpm`) and Flatpak.
