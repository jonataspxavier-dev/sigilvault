# SigilVault — Extensão do navegador / Browser extension

## Português

Esta pasta vai receber a extensão do SigilVault nas **Fases 4 e 5**:

- TypeScript com WebExtensions **Manifest V3**: um pacote para Chrome, Brave e Opera (Chromium) e outro, com ajustes no manifesto, para o Firefox.
- Conversa com o app desktop por **Native Messaging**, sem abrir porta de rede, depois de um pareamento seguro (PAKE) confirmado no app.
- **Não guarda senhas.** Ela só pede a credencial do site aberto, e só recebe se o cofre estiver desbloqueado.
- Preenche com um clique, nunca automaticamente, e compara a origem completa (https + domínio + porta).
- Balão "gerar senha forte ou só salvar" nos formulários de cadastro.

## English

This folder will hold the SigilVault extension in **Phases 4 and 5**:

- TypeScript WebExtensions **Manifest V3**: one package for Chrome, Brave and Opera (Chromium) and one, with manifest tweaks, for Firefox.
- Talks to the desktop app over **Native Messaging**, with no network port, after a secure pairing (PAKE) confirmed in the app.
- **Stores no passwords.** It only requests the credential for the current site, and only gets it while the vault is unlocked.
- Fills with one click, never automatically, and matches the full origin (https + domain + port).
- "Generate a strong password or just save" popup on sign-up forms.
