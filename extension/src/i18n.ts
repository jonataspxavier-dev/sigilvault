// Textos da extensão em português e inglês. O tipo de `en` obriga as duas línguas a terem as
// mesmas chaves. O idioma segue o do navegador.
// Extension strings in Portuguese and English. The type of `en` forces both languages to have the
// same keys. The language follows the browser's.

const pt = {
  status_unlocked: "Cofre aberto",
  status_unlocked_user: "Cofre de {name} aberto",
  status_locked: "Cofre trancado",
  status_locked_hint: "Desbloqueie no app para usar o SigilVault neste navegador.",
  open_app: "Abrir o SigilVault",
  connecting: "Conectando ao app…",
  retry: "Tentar de novo",

  pair_title: "Conectar ao app",
  pair_intro: "Para usar o SigilVault neste navegador, pareie a extensão com o app do computador. É preciso fazer isso uma vez só.",
  pair_button: "Parear com o app",
  code_intro: "Uma janela do SigilVault mostra um código de 8 caracteres. Digite-o aqui.",
  code_label: "Código do app",
  confirm: "Confirmar",
  cancel: "Cancelar",
  paired_ok: "Pareado! Este navegador agora conversa com o seu cofre.",

  unpair: "Desparear este navegador",
  unpair_confirm: "Confirmar: desparear",

  health_no_host: "O conector do SigilVault não foi encontrado. Instale o app SigilVault neste computador.",
  health_app_unavailable: "Não foi possível falar com o app SigilVault. Ele está instalado?",
  health_untrusted_app: "Conexão recusada por segurança: o programa encontrado não é o SigilVault original.",
  health_unknown_extension: "O app não reconheceu esta extensão. Use a extensão oficial do SigilVault.",

  err_locked: "O cofre está trancado. Desbloqueie o SigilVault e tente de novo.",
  err_busy: "Outro navegador está pareando agora. Tente daqui a pouco.",
  err_slow_down: "Aguarde alguns segundos e tente de novo.",
  err_too_many_pairings: "Limite de navegadores pareados atingido. Remova algum nas Configurações do app.",
  err_expired: "O código expirou. Comece de novo.",
  err_too_many_attempts: "Tentativas esgotadas. Comece de novo.",
  err_no_pairing: "O pedido de pareamento terminou. Comece de novo.",
  err_bad_code: "Código errado. Confira na janela do app.",
  err_malformed_code: "O código tem 8 letras e números, como ABCD-1234.",
  err_pairing_removed: "O pareamento foi removido no app. Pareie de novo para continuar.",
  err_generic: "Algo deu errado. Tente de novo.",

  footer: "Suas senhas ficam só no app do computador.",
  title_off: "SigilVault — sem conexão",
  title_locked: "SigilVault — cofre trancado",
  title_unlocked: "SigilVault — cofre aberto",
};

export type Key = keyof typeof pt;

const en: Record<Key, string> = {
  status_unlocked: "Vault unlocked",
  status_unlocked_user: "{name}'s vault is unlocked",
  status_locked: "Vault locked",
  status_locked_hint: "Unlock it in the app to use SigilVault in this browser.",
  open_app: "Open SigilVault",
  connecting: "Connecting to the app…",
  retry: "Try again",

  pair_title: "Connect to the app",
  pair_intro: "To use SigilVault in this browser, pair the extension with the desktop app. You only need to do this once.",
  pair_button: "Pair with the app",
  code_intro: "A SigilVault window shows an 8-character code. Type it here.",
  code_label: "Code from the app",
  confirm: "Confirm",
  cancel: "Cancel",
  paired_ok: "Paired! This browser now talks to your vault.",

  unpair: "Unpair this browser",
  unpair_confirm: "Confirm: unpair",

  health_no_host: "The SigilVault connector was not found. Install the SigilVault app on this computer.",
  health_app_unavailable: "Could not reach the SigilVault app. Is it installed?",
  health_untrusted_app: "Connection refused for safety: the program found is not the original SigilVault.",
  health_unknown_extension: "The app did not recognize this extension. Use the official SigilVault extension.",

  err_locked: "The vault is locked. Unlock SigilVault and try again.",
  err_busy: "Another browser is pairing right now. Try again shortly.",
  err_slow_down: "Wait a few seconds and try again.",
  err_too_many_pairings: "Paired browser limit reached. Remove one in the app's Settings.",
  err_expired: "The code expired. Start again.",
  err_too_many_attempts: "No attempts left. Start again.",
  err_no_pairing: "The pairing request ended. Start again.",
  err_bad_code: "Wrong code. Check the app window.",
  err_malformed_code: "The code has 8 letters and numbers, like ABCD-1234.",
  err_pairing_removed: "The pairing was removed in the app. Pair again to continue.",
  err_generic: "Something went wrong. Try again.",

  footer: "Your passwords stay in the desktop app only.",
  title_off: "SigilVault — not connected",
  title_locked: "SigilVault — vault locked",
  title_unlocked: "SigilVault — vault unlocked",
};

export const dictionaries = { pt, en };

export type Lang = keyof typeof dictionaries;

/** Idioma do navegador: português se começar com "pt", senão inglês. / Browser language: Portuguese if it starts with "pt", else English. */
export function detectLang(language: string | undefined): Lang {
  return language?.toLowerCase().startsWith("pt") ? "pt" : "en";
}

let lang: Lang = detectLang(typeof navigator !== "undefined" ? navigator.language : undefined);

export function setLang(l: Lang): void {
  lang = l;
}

/** Texto traduzido, com `{nome}` trocado pelos valores. / Translated text, with `{name}` replaced by values. */
export function t(key: Key, vars: Record<string, string> = {}): string {
  return dictionaries[lang][key].replace(/\{(\w+)\}/g, (_, k: string) => vars[k] ?? "");
}

/** Texto de um código de erro do app (ou genérico). / Text for an app error code (or generic). */
export function errorText(code: string): string {
  const key = `err_${code}` as Key;
  return key in pt ? t(key) : t("err_generic");
}
