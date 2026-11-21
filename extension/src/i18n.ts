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
  connecting: "Convocando o app…",
  retry: "Tentar de novo",

  pair_title: "Firmar pacto com o app",
  pair_intro: "Para usar o SigilVault neste navegador, firme um pacto (pareamento) com o app do computador. Basta fazer isso uma vez.",
  pair_button: "Parear com o app",
  code_intro: "Uma janela do SigilVault mostra um código de 8 caracteres. Digite-o aqui.",
  code_label: "Código do app",
  confirm: "Confirmar",
  cancel: "Cancelar",
  paired_ok: "Pacto firmado! Este navegador agora conversa com o seu cofre.",

  unpair: "Desfazer o pacto com este navegador",
  unpair_confirm: "Confirmar: desfazer o pacto",

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

  footer: "Seus segredos repousam só no app do computador.",
  title_off: "SigilVault — sem conexão",
  title_locked: "SigilVault — cofre trancado",
  title_unlocked: "SigilVault — cofre aberto",

  ui_icon: "Abrir o SigilVault",
  ui_close: "Fechar",
  ui_loading: "Consultando o guardião…",
  ui_not_paired: "Este navegador ainda não está pareado. Clique no ícone do SigilVault na barra do navegador para parear.",
  ui_no_app: "Não foi possível falar com o app SigilVault.",
  ui_locked: "O cofre está trancado.",
  ui_none: "Nenhum login salvo para {site}.",
  ui_pick: "Escolha a conta para preencher:",
  ui_insecure: "Atenção: esta página não é segura (http).",
  ui_cross_origin: "Atenção: este formulário está dentro de outro site.",
  ui_confirm_in_app: "Confirme no app SigilVault, se ele pedir…",
  ui_denied: "Preenchimento cancelado no app.",
  ui_not_allowed: "Este login não pertence a este site.",
  ui_rate_limited: "Muitos pedidos seguidos. Espere um minuto.",
  ui_no_fields: "Não encontrei os campos para preencher.",
  ui_tab_generate: "Forjar senha forte",
  ui_tab_save: "Só salvar",
  ui_length: "Tamanho",
  ui_upper: "Maiúsculas",
  ui_lower: "Minúsculas",
  ui_digits: "Números",
  ui_symbols: "Símbolos",
  ui_ambiguous: "Sem caracteres parecidos (l 1 O 0)",
  ui_bits: "{bits} bits",
  ui_strength_weak: "fraca",
  ui_strength_good: "forte",
  ui_strength_great: "muito forte",
  ui_strength_max: "excelente",
  ui_another: "Forjar outra",
  ui_use_save: "Usar e selar",
  ui_used_saved: "Senha forjada, preenchida e selada no cofre.",
  ui_just_save_intro: "Digite a sua senha no campo do site e clique em Salvar. O SigilVault só guarda.",
  ui_just_save_button: "Selar no cofre",
  ui_saved: "Selado no cofre.",
  ui_updated: "Senha atualizada. A anterior ficou guardada no histórico.",
  ui_already: "Este login já está salvo, igualzinho.",
  ui_empty_password: "Digite a senha no campo do site primeiro.",
  ui_save_new: "Salvar o login de {user} em {site}?",
  ui_save_new_nouser: "Salvar este login em {site}?",
  ui_save_changed: "Atualizar a senha de \"{title}\" ({user})?",
  ui_save_history: "A senha antiga fica guardada no histórico.",
  ui_save_button: "Salvar",
  ui_update_button: "Atualizar",
  ui_not_now: "Agora não",
  ui_generic: "Algo deu errado. Tente de novo.",
};

export type Key = keyof typeof pt;

const en: Record<Key, string> = {
  status_unlocked: "Vault unlocked",
  status_unlocked_user: "{name}'s vault is unlocked",
  status_locked: "Vault locked",
  status_locked_hint: "Unlock it in the app to use SigilVault in this browser.",
  open_app: "Open SigilVault",
  connecting: "Summoning the app…",
  retry: "Try again",

  pair_title: "Seal a pact with the app",
  pair_intro: "To use SigilVault in this browser, seal a pact (pairing) with the desktop app. You only need to do this once.",
  pair_button: "Pair with the app",
  code_intro: "A SigilVault window shows an 8-character code. Type it here.",
  code_label: "Code from the app",
  confirm: "Confirm",
  cancel: "Cancel",
  paired_ok: "Pact sealed! This browser now talks to your vault.",

  unpair: "Undo the pact with this browser",
  unpair_confirm: "Confirm: undo the pact",

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

  footer: "Your secrets rest only in the desktop app.",
  title_off: "SigilVault — not connected",
  title_locked: "SigilVault — vault locked",
  title_unlocked: "SigilVault — vault unlocked",

  ui_icon: "Open SigilVault",
  ui_close: "Close",
  ui_loading: "Asking the keeper…",
  ui_not_paired: "This browser is not paired yet. Click the SigilVault icon in the browser toolbar to pair.",
  ui_no_app: "Could not reach the SigilVault app.",
  ui_locked: "The vault is locked.",
  ui_none: "No saved login for {site}.",
  ui_pick: "Pick the account to fill:",
  ui_insecure: "Warning: this page is not secure (http).",
  ui_cross_origin: "Warning: this form is inside another site.",
  ui_confirm_in_app: "Confirm in the SigilVault app, if it asks…",
  ui_denied: "Fill cancelled in the app.",
  ui_not_allowed: "This login does not belong to this site.",
  ui_rate_limited: "Too many requests in a row. Wait a minute.",
  ui_no_fields: "Could not find the fields to fill.",
  ui_tab_generate: "Forge a strong password",
  ui_tab_save: "Just save",
  ui_length: "Length",
  ui_upper: "Uppercase",
  ui_lower: "Lowercase",
  ui_digits: "Digits",
  ui_symbols: "Symbols",
  ui_ambiguous: "No look-alike characters (l 1 O 0)",
  ui_bits: "{bits} bits",
  ui_strength_weak: "weak",
  ui_strength_good: "strong",
  ui_strength_great: "very strong",
  ui_strength_max: "excellent",
  ui_another: "Forge another",
  ui_use_save: "Use and seal",
  ui_used_saved: "Password forged, filled in and sealed in the vault.",
  ui_just_save_intro: "Type your password in the site's field and click Save. SigilVault only stores it.",
  ui_just_save_button: "Seal in the vault",
  ui_saved: "Sealed in the vault.",
  ui_updated: "Password updated. The previous one was kept in the history.",
  ui_already: "This login is already saved, exactly the same.",
  ui_empty_password: "Type the password in the site's field first.",
  ui_save_new: "Save the login of {user} on {site}?",
  ui_save_new_nouser: "Save this login on {site}?",
  ui_save_changed: "Update the password of \"{title}\" ({user})?",
  ui_save_history: "The old password is kept in the history.",
  ui_save_button: "Save",
  ui_update_button: "Update",
  ui_not_now: "Not now",
  ui_generic: "Something went wrong. Try again.",
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
