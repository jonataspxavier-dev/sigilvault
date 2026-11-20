# SigilVault — Planejamento do Projeto

Versão de 20/11/2026.

## Visão geral

O SigilVault é um gerenciador de senhas local para Windows: guarda tudo criptografado no próprio computador, preenche senhas no navegador e em apps desktop e gera senhas fortes. Nada vai para a nuvem, a não ser que o usuário escolha.

Princípios do projeto:

- **Segurança em primeiro lugar.** Tudo é salvo criptografado e só é descriptografado na memória, na hora de usar.
- **Local-first.** Sem servidor, sem conta online, sem telemetria. O usuário controla o próprio cofre.
- **Leveza.** O app fica aberto o dia todo na bandeja do Windows, consumindo o mínimo de memória e CPU.
- **Senhas fortes como hábito.** Toda senha nova, no navegador ou no desktop, nasce do gerador.
- **Identidade própria.** Nome, ícones e visual 100% originais.

Primeira plataforma: Windows 10 e 11. Linux e macOS ficam para depois. O repositório começa privado no GitHub e os commits ficam somente no nome do autor do projeto (sem coautoria).

Idiomas: a interface do app e da extensão é bilíngue (português e inglês), com troca de idioma nas configurações. Commits e comentários do código também são bilíngues.

## Arquitetura

O app desktop é o único lugar que guarda e descriptografa senhas. A extensão só pede a credencial do site aberto, e só recebe se o cofre estiver desbloqueado.

```
┌──────────── Navegador ────────────┐            ┌──────────── App SigilVault no Windows ────────────┐
│  Extensão SigilVault              │  Native    │  Serviço da bandeja (sempre ativo, ~10–20 MB)     │
│  - detecta formulários            │◄──────────►│  - atende a extensão e os atalhos globais         │
│  - preenche com 1 clique          │ Messaging  │  - Auto-Type nos apps desktop                     │
│  - NÃO guarda senhas              │ (pareado)  │  - bloqueio automático                            │
└───────────────────────────────────┘            │                     ▲▼                            │
                                                 │  Núcleo Rust: o ÚNICO que descriptografa          │
┌─ Apps desktop ─┐   Auto-Type (via atalho)      │  - Argon2id · XChaCha20-Poly1305 · slots de chave │
│ recebem usuário│◄──────────────────────────────│  - gerador de senhas · códigos de recuperação     │
│ e senha        │                               │  - chaves apagadas da memória ao bloquear         │
└────────────────┘                               │                     ▲▼                            │
┌─ cofre.sigil ──┐        lê e grava             │  Janela do app (Tauri, ~50–80 MB)                 │
│ criptografado  │◄─────────────────────────────►│  - buscar, ver, editar e gerar senhas             │
│ em disco       │                               │  - carregada só quando aberta                     │
└────────────────┘                               └───────────────────────────────────────────────────┘
```

O app tem duas partes: o serviço da bandeja fica sempre ativo e leve, e a janela só é carregada quando aberta. O Native Messaging não abre porta de rede, e o pareamento impede que uma extensão falsa peça senhas.

## Stack e navegadores

Escolhemos Tauri 2 com núcleo em Rust, porque é a opção mais leve para um app que fica aberto o tempo todo, e Rust é forte em segurança de memória e criptografia.

| Parte | Tecnologia | Por quê |
| --- | --- | --- |
| Núcleo (cofre, criptografia, gerador) | Rust | Seguro, rápido, sem falhas de memória comuns em C/C++ |
| App desktop | Tauri 2 + interface em TypeScript | Usa o WebView2 que já vem no Windows; instalador de ~5–10 MB |
| Extensão | TypeScript (WebExtensions, Manifest V3) | Único código para todos os navegadores |
| Comunicação extensão ↔ app | Native Messaging | Canal oficial dos navegadores, sem porta de rede aberta |

Consumo esperado de memória:

- **Janela fechada, só na bandeja:** ~10–20 MB, CPU praticamente zero.
- **Janela aberta:** ~50–80 MB. Ao fechar, a interface é descarregada.
- Para comparação, Electron ficaria em ~150–300 MB o tempo todo.

Navegadores alvo: **Chrome, Brave, Opera e Firefox**. Os três primeiros usam a base Chromium e compartilham o mesmo pacote. O Firefox recebe um pacote com pequenos ajustes no manifesto.

## Criptografia e segurança

Regra de ouro: nunca inventar criptografia própria. Usamos só bibliotecas auditadas do ecossistema Rust (RustCrypto ou libsodium).

| Etapa | Como é feito |
| --- | --- |
| Senha mestra → chave | Argon2id com memória alta (64–256 MB), deixando a força bruta muito cara |
| Chave do cofre | Chave aleatória de 256 bits, guardada criptografada em "slots" (senha mestra e códigos de recuperação) |
| Dados | XChaCha20-Poly1305 (criptografia autenticada: detecta adulteração) |
| Aleatoriedade | Sempre o gerador seguro do sistema operacional |

Proteções do dia a dia:

- Bloqueio automático por inatividade, ao bloquear o Windows e ao suspender o PC.
- Área de transferência limpa sozinha ~30 segundos depois de copiar uma senha.
- Chaves e senhas apagadas da memória ao bloquear (zeroize).
- Desbloqueio rápido opcional com Windows Hello.
- Pareamento entre extensão e app com confirmação do usuário, para que uma extensão falsa não consiga pedir senhas.
- Limite de tentativas de desbloqueio com espera crescente.

Modelo de ameaça:

- **Protege contra:** roubo do notebook ou do HD, vazamento do arquivo exportado ou do backup, sites falsos (phishing), vazamento de nuvem.
- **Não protege contra:** vírus ou keylogger rodando no PC enquanto o cofre está desbloqueado. Essa parte depende do bom uso do computador pelo usuário.

## Conta local e recuperação

O acesso ao cofre usa usuário + senha mestra, e o usuário recebe 10 códigos de recuperação de uso único para quando esquecer a senha ou o usuário.

1. **Criação do cofre:** o usuário define o nome de usuário e a senha mestra. O app gera 10 códigos (ex.: `K7QM-2XPA-9RTD-H4WN-6CZE-BJ3T-Q`), cada um com pelo menos 128 bits de aleatoriedade.
2. **Como funciona por dentro:** a chave do cofre fica guardada em 11 cópias criptografadas (slots): uma pela senha mestra e uma por código. Qualquer slot abre o cofre.
3. **Esqueceu a senha ou o usuário:** um código sozinho abre o cofre. O app mostra o nome de usuário, obriga a criar uma senha mestra nova e destrói aquele código, mostrando quantos restam ("restam 9").
4. **Regenerar:** a qualquer momento dá para gerar 10 códigos novos, invalidando os antigos.
5. **Trocar a senha mestra:** os códigos continuam valendo, porque protegem a chave do cofre, não a senha.

Cuidados que o app deve deixar claros na tela:

- Cada código vale tanto quanto a senha mestra. A tela dos códigos orienta a imprimir ou anotar e guardar fora do computador, nunca num arquivo de texto.
- O nome de usuário identifica e organiza o cofre (vários perfis no mesmo PC), mas não é um fator de segurança. Ele não entra na criptografia, para que esquecê-lo nunca tranque o cofre.
- Sem senha mestra e sem nenhum código, o cofre não pode ser recuperado por ninguém. Isso é o preço de não ter servidor.

## Armazenamento, exportação e importação

O cofre é um único arquivo criptografado, e exportar é copiar esse arquivo: ele já sai ilegível para quem não tem a senha mestra ou um código.

- **Arquivo do cofre:** `cofre.sigil`, guardado na pasta de dados do usuário (AppData). O cabeçalho tem a versão do formato, para permitir migrações futuras.
- **Exportar:** gera uma cópia criptografada. Opcionalmente, com uma senha só para aquele arquivo de exportação.
- **Importar em outro PC:** instala o SigilVault, importa o arquivo e desbloqueia com a senha mestra (ou com um código de recuperação).
- **Nuvem opcional, sem confiar na nuvem:** o usuário pode guardar o arquivo no Drive ou OneDrive dele. A nuvem só vê dados criptografados.
- **Backups automáticos locais:** cópias rotativas do arquivo (ex.: as 5 últimas) antes de cada alteração importante.
- **CSV legível:** só como opção avançada, com aviso forte e confirmação da senha mestra.
- **Importar de outros gerenciadores:** Chrome, Firefox, Bitwarden e KeePass, para facilitar a migração.

## Funcionalidades

O SigilVault faz quatro coisas: preenche logins conhecidos, ajuda a criar contas com senhas fortes (gerar ou só salvar), gera senhas sob medida e permite buscar e editar o cofre.

### Preencher logins no navegador

- A extensão detecta formulários de login e mostra o ícone do SigilVault no campo.
- Preenche com um clique, nunca automaticamente ao carregar a página. Isso evita que formulários escondidos roubem senhas.
- Correspondência pela origem completa (https + domínio + porta): `g00gle.com` nunca recebe a senha do `google.com`.
- Várias contas no mesmo site aparecem numa listinha para escolher.

### Criar conta ou trocar senha: gerar ou só salvar

1. A extensão reconhece um formulário de cadastro ou troca de senha (usuário ou e-mail + senha, às vezes com "confirmar senha").
2. Ao clicar no campo de senha, abre o balão do SigilVault com duas opções:
   - **Gerar senha forte:** mostra ali mesmo as opções (tamanho, símbolos, números, maiúsculas), uma prévia com indicador de força e o botão "Gerar outra".
   - **Só salvar:** o usuário digita a própria senha e o SigilVault apenas guarda.
3. "Usar e salvar" preenche a senha e a confirmação e já guarda usuário, senha e site no cofre.
4. Rede de segurança: se o formulário for enviado sem usar o balão, aparece "Salvar essa senha no SigilVault?".
5. As opções preferidas do gerador ficam salvas como padrão.

### Preencher e gerar no desktop

- **Atalho global de busca** (ex.: `Ctrl+Shift+Espaço`): abre um pop-up rápido para buscar uma conta, copiar ou digitar a senha.
- **Atalho de gerar** (ex.: `Ctrl+Shift+G`): abre o mesmo balão "gerar ou só salvar", digita no campo ativo e oferece salvar com o nome do programa.
- **Auto-Type:** o app reconhece a janela ativa (título e processo), sugere a credencial certa e digita usuário + Tab + senha.
- **Fase posterior:** o balão aparecer sozinho ao clicar num campo de senha de outro programa, via UI Automation do Windows. É a parte mais difícil do projeto.

### Gerador de senhas

- Opções: tamanho (padrão 24–32, máximo 128), maiúsculas, minúsculas, números e símbolos (cada um liga/desliga) e excluir caracteres ambíguos (`l`, `1`, `O`, `0`).
- Garante pelo menos um caractere de cada tipo escolhido e sorteia sem viés (rejection sampling).
- Mostra a força em bits de entropia. Com todos os tipos, 20 caracteres já dão ~130 bits.
- Modo frase-senha (ex.: `cavalo-trilho-nuvem-sete-farol-mesa`), com 6 palavras por padrão, ótimo para a senha mestra.
- Muitos sites limitam a 64 caracteres ou menos, por isso o padrão não é o máximo.

### Gerenciar o cofre

- Busca instantânea por site, app, usuário ou etiqueta.
- Ver senha (com reconfirmação opcional), copiar, editar e excluir.
- Histórico das senhas anteriores de cada conta.
- Itens além de logins: notas seguras e, mais adiante, cartões e documentos.
- Relatório de saúde: senhas fracas, repetidas ou antigas.

## Identidade visual

O visual do SigilVault segue um estilo de fantasia medieval nórdica, inspirado na atmosfera de Skyrim: pedra, ferro forjado, couro, cera de selo e runas. Tudo é desenhado por nós. É uma inspiração de clima, sem copiar nada do jogo, para o app ter cara própria e evitar problemas de direitos.

- **Nome:** SigilVault. *Sigil* = selo ou sinete (do latim *sigillum*, mesma origem de "sigilo"). *Vault* = cofre.
- **Ícone principal (logo):** um baú-cofre de ferro forjado com reforços rebitados, uma fechadura ornamentada no centro e um selo de cera com o "S" do SigilVault lacrando a tampa. Mistura de cofre, fechadura e selo.
- **Clima visual:**
  - cores: cinza-pedra e ferro escuro, couro marrom, vermelho-cera nos selos e um dourado envelhecido como destaque
  - texturas leves de pedra, pergaminho e metal escovado, sem pesar a interface
  - detalhes de runas e ornamentos nórdicos criados por nós, nas bordas e separadores
  - tipografia: uma fonte serifada de estilo antigo (licença livre) nos títulos e uma fonte limpa e legível no texto e nas senhas
- **Ícones próprios no mesmo estilo:** desenhados em SVG, com a mesma espessura de traço e ornamentos. Nada de Font Awesome, Material Icons ou similares.
  - logo do app e ícone da bandeja (versões claro e escuro)
  - ícone da extensão na barra do navegador: fechadura trancada (bloqueado) e aberta (desbloqueado)
  - ícone dentro dos campos de login: um pequeno selo
  - ações: gerar (runa ou pena), salvar (selo de cera), copiar (pergaminho duplo), ver/ocultar (olho ornamentado), editar, excluir, buscar (lupa de latão), bloquear (cadeado de ferro), exportar e importar (baú com seta)
  - tipos de item: login (chave antiga), nota segura (pergaminho), app desktop (escudo)
- **Limites da referência ao jogo:** nada de logo, símbolo do dragão, fontes, runas, imagens ou nomes do Skyrim. A Bethesda é dona dessa marca; o SigilVault só compartilha o clima medieval.
- **Legibilidade em primeiro lugar:** o tema é decoração; senhas, campos e botões continuam claros, com bom contraste. Os ícones precisam ser reconhecíveis mesmo a 16 px.
- **Tamanhos:** 16, 32, 48 e 128 px para a extensão; arquivo `.ico` com vários tamanhos para o Windows.
- **Próximo passo:** esboçar 2 ou 3 versões do logo e uma folha com todos os ícones lado a lado para conferir a consistência.

## Ajustes da revisão de segurança

A revisão adversarial de 20/11/2026 encontrou 24 pontos, 3 deles críticos, e todos entram no projeto. Onde este capítulo contradiz outro, vale este.

### Comunicação extensão ↔ app (crítico)

- O processo do Native Messaging fala com o serviço da bandeja por um named pipe protegido: acesso só para o usuário atual (DACL explícita), nome que não pode ser "sequestrado" por outro programa (FILE_FLAG_FIRST_PIPE_INSTANCE), e os dois lados conferem o executável e a assinatura digital (Authenticode) um do outro.
- Pareamento com troca de chaves forte (PAKE) e confirmação no app. O segredo de pareamento é tratado como credencial.
- O primeiro preenchimento em cada site novo pede confirmação no app desktop. Consultas têm limite de frequência, a extensão nunca pode "listar todas as senhas" e o app mostra um registro dos preenchimentos.
- Limite assumido: um programa malicioso rodando como o mesmo usuário não pode ser totalmente bloqueado (fica fora do escopo, como o keylogger), mas essas camadas dificultam muito.

### Códigos de recuperação e chave do cofre (crítico)

- Ao usar um código, regenerar os códigos ou trocar a senha mestra, a chave do cofre é trocada e todos os dados são recriptografados, inclusive os backups locais.
- Cópias que já saíram do app (exportações, nuvem, outro PC) não podem ser revogadas. O app avisa isso na tela.
- Códigos com 128 bits de verdade: 26 ou mais caracteres em base32 com dígito verificador, chave derivada com HKDF e um identificador para achar o slot certo sem testar os 10.
- Exportação com senha própria sai sem os slots dos códigos.

### Formato do arquivo e integridade

- Cabeçalho inteiro (versão, parâmetros do Argon2, slots) autenticado, e cada item amarrado ao seu ID e campo (AAD). Isso impede, por exemplo, trocar a senha do banco para outro site.
- Parâmetros mínimos do Argon2 obrigatórios, com memória, iterações e paralelismo registrados por slot.
- Proteção contra "voltar versão": um contador de versão também fica guardado fora do cofre (DPAPI), e o app avisa se o arquivo ficar mais antigo.
- Todo o conteúdo é criptografado, inclusive usuário, sites e títulos. O nome do perfil na tela de login fica num índice local que nunca é exportado.
- Gravação atômica (arquivo temporário, FlushFileBuffers, depois MoveFileEx) e tratamento de conflito quando o mesmo cofre é editado em dois PCs.

### Senha mestra e gerador

- Senha mestra fraca é bloqueada (avaliação com zxcvbn), com recomendação de frase-senha de 6 ou mais palavras.
- Gerador: se a senha sorteada não tiver todos os tipos escolhidos, sorteia de novo inteira, sem viés. A entropia é calculada depois das exclusões.
- Frase-senha: lista fixa de pelo menos 7.776 palavras sem acentos, padrão de 6 palavras.

### Navegador

- Preenchimento pela origem completa por padrão; subdomínios só se o usuário permitir. Nunca preenche em página http uma senha salva em https.
- A origem vem do próprio navegador (sender.origin no service worker), nunca da página. iframes de outro site não recebem preenchimento sem aprovação. Domínios com caracteres estranhos aparecem em punycode.
- Ícone e balão ficam num iframe isolado da extensão, que a página não consegue ler nem cobrir (proteção contra clickjacking). Só reagem a cliques reais (isTrusted) e preenchem só os campos visíveis do formulário em foco.
- Salvar ou alterar só com ação real do usuário. Nunca sobrescreve sem mostrar a diferença, e o histórico fica guardado.

### Desktop

- Auto-Type identifica o programa pelo caminho completo do executável e pela assinatura, nunca pelo título da janela.
- Não digita em navegadores (lá quem preenche é a extensão).
- Confere a janela (HWND e PID) antes de cada bloco de digitação e cancela se ela mudar.
- Usa digitação Unicode (KEYEVENTF_UNICODE), que resolve os acentos do teclado ABNT2, e detecta janelas de administrador (UIPI).
- O atalho de gerar salva a entrada como "pendente" antes de digitar.
- Atalhos configuráveis, com aviso se a combinação já estiver em uso.

### Área de transferência, memória e bloqueio

- Senhas copiadas não entram no histórico do Windows (Win+V) nem na Área de Transferência na nuvem (formatos ExcludeClipboardContentFromMonitorProcessing, CanIncludeInClipboardHistory=0, CanUploadToCloudClipboard=0). A limpeza só acontece se o conteúdo ainda for a senha (número de sequência), e também ao bloquear.
- Segredos ficam em memória travada (VirtualLock), sem cópias desnecessárias. A janela (WebView2) é destruída ao bloquear, e senhas só vão para a interface quando o usuário pede para ver.
- Processo excluído dos relatórios de erro do Windows (WER) com dump de memória. Recomendar BitLocker ao usuário.
- Bloqueio também ao desconectar sessão remota ou console, trocar de usuário e no Modern Standby (WTS_SESSION_LOCK, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT). O tempo é medido por relógio monotônico, e o contador de tentativas fica salvo em disco.

### Interface (Tauri) e importação

- Política de segurança de conteúdo (CSP) estrita, sem innerHTML, e permissões mínimas do Tauri por janela.
- Exportar e "mostrar todas" exigem confirmação num diálogo nativo do Windows.
- Importadores testados com entradas malformadas (fuzzing). CSV gravado só em pasta escolhida pelo usuário, com aviso sobre OneDrive, e oferta de apagar o CSV importado.

### Windows Hello, instalação e privacidade

- Windows Hello com chave presa ao TPM (KeyCredentialManager), guardada fora do cofre (nunca exportada). A senha mestra é exigida após reiniciar e a cada N dias. Fica para as fases 6 e 7.
- Instalação em Program Files (por máquina) com atualizações assinadas, para impedir que outro programa troque o executável.
- Telemetria do WebView2 desligada; verificação de atualização só se o usuário ativar.

## Fases de desenvolvimento

Sete fases, cada uma termina com algo funcionando e testado antes de passar para a próxima.

1. **Núcleo em Rust + testes:** criar, abrir e salvar o cofre; Argon2id; slots de chave; 10 códigos de recuperação; gerador de senhas. Sem interface ainda.
   - Pronto quando: os testes automatizados passam, incluindo senha errada, código usado duas vezes e arquivo adulterado.
2. **App desktop (Tauri):** tela de criação e desbloqueio, listar, buscar, criar, editar e excluir; gerador na interface; bandeja e bloqueio automático.
3. **Exportar e importar:** exportação criptografada, importação em outro PC, backups locais e importação de outros gerenciadores.
4. **Extensão + Native Messaging:** comunicação com o app e pareamento seguro, nos quatro navegadores.
5. **Navegador completo:** preencher logins, balão "gerar ou só salvar" e aviso de salvar após envio.
6. **Desktop completo:** atalhos globais, pop-up de busca, Auto-Type e o balão "gerar ou só salvar" via atalho.
7. **Endurecimento e lançamento:** revisão de segurança, testes de ataque, ícones finais, instalador assinado e publicação da extensão nas lojas.

Os ícones originais são criados em paralelo: os básicos na fase 2, os da extensão na fase 4 e o conjunto final na fase 7.

## Ambiente de trabalho

O desenvolvimento acontece no VS Code do Windows, com o Claude Code integrado. O autor instala só Git, VS Code e a extensão Claude Code; o resto o Claude instala, pedindo aprovação a cada comando.

### O que o Claude instala

| Ferramenta | Para quê | Comando |
| --- | --- | --- |
| Node.js LTS | Interface do app e extensão | `winget install OpenJS.NodeJS.LTS` |
| Rust (rustup) | Núcleo e app Tauri | `winget install Rustlang.Rustup` |
| Visual Studio Build Tools (C++) | Compilador que o Rust usa no Windows | `winget install Microsoft.VisualStudio.2022.BuildTools` com a carga "Desktop C++" |
| WebView2 | Interface do Tauri | Já vem no Windows 10 e 11 |

O Build Tools é grande (alguns GB) e pode pedir reinicialização.

### Regras do repositório

- Repositório **privado** `sigilvault` no GitHub do autor.
- Commits **só no nome do autor**, sem linha de coautoria (desativar nas configurações do Claude Code do projeto).
- Tudo bilíngue (português e inglês): mensagens de commit no formato "pt-BR / en" e comentários do código nas duas línguas, português primeiro. Commits pequenos, pensando no estudo do autor.
- Nunca subir senhas, chaves ou arquivos de cofre reais (o `.gitignore` bloqueia `*.sigil`).
