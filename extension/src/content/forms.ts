// Detecção de formulários de login e de cadastro, e preenchimento dos campos.
// Não usa APIs de extensão: pode ser testado com um DOM de mentira.
// Login and sign-up form detection, and field filling. Uses no extension APIs: it can be tested
// with a fake DOM.

export type FormKind = "login" | "signup";

/** O formulário em volta de um campo. / The form around a field. */
export interface FormContext {
  /** `<form>` ou o bloco mais próximo que contém os campos. / `<form>` or the nearest block containing the fields. */
  container: Element;
  username: HTMLInputElement | null;
  /** Campos de senha visíveis, na ordem da página. / Visible password fields, in page order. */
  passwords: HTMLInputElement[];
  kind: FormKind;
}

export type Visible = (el: HTMLElement) => boolean;

/** Visível de verdade (tamanho, CSS, opacidade). / Truly visible (size, CSS, opacity). */
export const isVisible: Visible = (el) => {
  if (!el.isConnected) return false;
  const check = (el as HTMLElement & { checkVisibility?: (o: object) => boolean }).checkVisibility;
  if (typeof check === "function" && !check.call(el, { opacityProperty: true, visibilityProperty: true, contentVisibilityAuto: true })) return false;
  const r = el.getBoundingClientRect();
  return r.width > 1 && r.height > 1;
};

const TEXT_TYPES = new Set(["", "text", "email", "tel"]);
const USER_HINT = /user|login|e-?mail|account|conta|usu[aá]rio|cpf|identif/i;
const SIGNUP_HINT = /regist|sign.?up|cadastr|criar.?conta|create.?account|join|new.?password|nova.?senha|alterar.?senha|change.?password|reset/i;

function usable(el: HTMLInputElement, visible: Visible): boolean {
  return !el.disabled && !el.readOnly && visible(el);
}

export function isPasswordField(el: Element): el is HTMLInputElement {
  return el instanceof HTMLInputElement && el.type === "password";
}

export function isUsernameLike(el: Element): el is HTMLInputElement {
  if (!(el instanceof HTMLInputElement) || !TEXT_TYPES.has(el.getAttribute("type")?.toLowerCase() ?? "")) return false;
  const ac = el.autocomplete.toLowerCase();
  if (ac.includes("username") || ac.includes("email") || el.type === "email") return true;
  return USER_HINT.test(`${el.name} ${el.id} ${el.placeholder} ${el.getAttribute("aria-label") ?? ""}`);
}

/** Sobe do campo até achar o `<form>` ou um bloco com campo de senha. / Climbs from the field to the `<form>` or a block with a password field. */
function containerOf(field: HTMLInputElement): Element {
  if (field.form) return field.form;
  let node: Element | null = field.parentElement;
  for (let depth = 0; node && depth < 8; depth++, node = node.parentElement) {
    if (node.querySelector('input[type="password"]') && node.querySelectorAll("input").length >= (isPasswordField(field) ? 2 : 1)) return node;
  }
  return field.parentElement ?? field;
}

function signals(container: Element): string {
  const attrs = ["id", "name", "class", "action", "aria-label"].map((a) => container.getAttribute(a) ?? "").join(" ");
  const buttons = [...container.querySelectorAll('button, input[type="submit"]')]
    .map((b) => (b instanceof HTMLInputElement ? b.value : (b.textContent ?? "")))
    .join(" ")
    .slice(0, 400);
  return `${attrs} ${buttons}`;
}

/** Contexto do formulário de um campo, ou null se não for login/cadastro. / Form context of a field, or null if not login/sign-up. */
export function contextFor(field: Element, visible: Visible = isVisible): FormContext | null {
  if (!(field instanceof HTMLInputElement)) return null;
  if (!isPasswordField(field) && !isUsernameLike(field)) return null;
  const container = containerOf(field);
  const inputs = [...container.querySelectorAll("input")].filter((i) => usable(i, visible));
  const passwords = inputs.filter(isPasswordField);
  // Só usuário (login em duas etapas): aceita se o campo diz claramente que é usuário.
  // Username only (two-step login): accepted if the field clearly says it is a username.
  if (passwords.length === 0) {
    const ac = field.autocomplete.toLowerCase();
    if (!ac.includes("username") && field.type !== "email") return null;
    return { container, username: field, passwords: [], kind: "login" };
  }
  const firstPassword = inputs.indexOf(passwords[0]!);
  const before = inputs.slice(0, firstPassword).filter((i) => TEXT_TYPES.has(i.getAttribute("type")?.toLowerCase() ?? ""));
  const username =
    inputs.find((i) => i.autocomplete.toLowerCase().includes("username")) ??
    (isPasswordField(field) ? null : field) ??
    [...before].reverse().find(isUsernameLike) ??
    before[before.length - 1] ??
    null;
  const signup =
    passwords.length >= 2 || passwords.some((p) => p.autocomplete.toLowerCase().includes("new-password")) || SIGNUP_HINT.test(signals(container));
  return { container, username, passwords, kind: signup ? "signup" : "login" };
}

/** Define o valor como se o usuário tivesse digitado (React, Vue e afins percebem).
 *  Sets the value as if the user had typed it (React, Vue and the like notice). */
export function setValue(el: HTMLInputElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  if (setter) setter.call(el, value);
  else el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

/** Campos de senha NOVA (cadastro/troca): pula o de senha atual. / NEW password fields (sign-up/change): skips the current one. */
export function newPasswordFields(ctx: FormContext): HTMLInputElement[] {
  const fresh = ctx.passwords.filter((p) => !p.autocomplete.toLowerCase().includes("current-password"));
  // Troca com 3 campos (atual, nova, confirmar) sem dica: o primeiro é o atual.
  // Change with 3 fields (current, new, confirm) without hints: the first is the current one.
  if (fresh.length === 3 && ctx.passwords.length === 3) return fresh.slice(1);
  return fresh;
}

/** Preenche um login. Só campos ainda visíveis. Devolve quantos campos preencheu.
 *  Fills a login. Only fields still visible. Returns how many fields were filled. */
export function fillLogin(ctx: FormContext, username: string, password: string, visible: Visible = isVisible): number {
  let n = 0;
  if (ctx.username && username && usable(ctx.username, visible)) {
    setValue(ctx.username, username);
    n++;
  }
  const pw = ctx.passwords.find((p) => usable(p, visible));
  if (pw) {
    setValue(pw, password);
    n++;
  }
  return n;
}

/** Preenche a senha nova e a confirmação. / Fills the new password and its confirmation. */
export function fillNewPassword(ctx: FormContext, password: string, visible: Visible = isVisible): number {
  const fields = newPasswordFields(ctx).filter((p) => usable(p, visible));
  fields.forEach((f) => setValue(f, password));
  return fields.length;
}

/** O que está digitado no formulário (para salvar). / What is typed in the form (to save). */
export function readCredentials(ctx: FormContext): { username: string; password: string } {
  const pwField = ctx.kind === "signup" ? (newPasswordFields(ctx)[0] ?? ctx.passwords[0]) : ctx.passwords[0];
  return { username: ctx.username?.value.trim() ?? "", password: pwField?.value ?? "" };
}
