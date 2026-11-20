// Componentes reutilizáveis da interface.
// Reusable UI components.

import { api, isCmdError } from "./api";
import { append, h, type Child } from "./dom";
import { hasKey, t } from "./i18n";
import { icon, type IconName } from "./icons";
import { charClass, strengthLabel } from "./util";

type Variant = "primary" | "secondary" | "ghost" | "danger";

export function button(
  label: string,
  opts: { icon?: IconName; variant?: Variant; onClick?: () => void; type?: "button" | "submit"; title?: string; iconOnly?: boolean } = {},
): HTMLButtonElement {
  const b = h("button", {
    type: opts.type ?? "button",
    class: `btn btn-${opts.variant ?? "secondary"}${opts.iconOnly ? " btn-icon" : ""}`,
    title: opts.title ?? (opts.iconOnly ? label : undefined),
    "aria-label": opts.iconOnly ? label : undefined,
    onClick: opts.onClick,
  });
  if (opts.icon) b.appendChild(icon(opts.icon));
  if (!opts.iconOnly) append(b, h("span", null, label));
  return b;
}

/** Campo com rótulo e dica opcional. / Field with label and optional hint. */
export function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  return h("label", { class: "field" }, h("span", { class: "field-label" }, label), control, hint ? h("span", { class: "field-hint" }, hint) : null);
}

export function textInput(opts: { value?: string; placeholder?: string; autocomplete?: string; maxLength?: number } = {}): HTMLInputElement {
  return h("input", {
    type: "text",
    class: "input",
    value: opts.value ?? "",
    placeholder: opts.placeholder,
    autocomplete: opts.autocomplete ?? "off",
    spellcheck: false,
    maxLength: opts.maxLength ?? 512,
  });
}

/** Campo de senha com botão de mostrar/ocultar. / Password field with show/hide button. */
export function passwordInput(opts: { placeholder?: string; autocomplete?: string } = {}): { wrap: HTMLElement; input: HTMLInputElement } {
  const input = h("input", {
    type: "password",
    class: "input input-mono",
    placeholder: opts.placeholder,
    autocomplete: opts.autocomplete ?? "off",
    spellcheck: false,
    maxLength: 1024,
  });
  const toggle = h("button", { type: "button", class: "input-addon", title: t("show"), "aria-label": t("show") }, icon("eye"));
  toggle.addEventListener("click", () => {
    const show = input.type === "password";
    input.type = show ? "text" : "password";
    toggle.replaceChildren(icon(show ? "eyeOff" : "eye"));
    toggle.title = t(show ? "hide" : "show");
  });
  return { wrap: h("div", { class: "input-group" }, input, toggle), input };
}

/** Medidor de força da senha mestra (zxcvbn no Rust). / Master password strength meter (zxcvbn in Rust). */
export function strengthMeter(getUsername: () => string): { el: HTMLElement; watch(input: HTMLInputElement): void; score(): number } {
  const bars = Array.from({ length: 5 }, () => h("span", { class: "meter-bar" }));
  const label = h("span", { class: "meter-label" }, "");
  const el = h("div", { class: "meter", "data-score": "-1" }, h("div", { class: "meter-bars" }, ...bars), label);
  let current = -1;
  let timer: number | undefined;
  return {
    el,
    score: () => current,
    watch(input) {
      input.addEventListener("input", () => {
        window.clearTimeout(timer);
        timer = window.setTimeout(async () => {
          if (!input.value) {
            current = -1;
            el.dataset.score = "-1";
            label.textContent = "";
            return;
          }
          current = await api.passwordStrength(input.value, getUsername());
          el.dataset.score = String(current);
          label.textContent = t(strengthLabel(current));
          input.dispatchEvent(new Event("strength"));
        }, 150);
      });
    },
  };
}

/** Senha colorida por tipo de caractere (facilita ler e digitar). / Password colored by char type (easier to read and type). */
export function coloredSecret(secret: string): HTMLElement {
  const el = h("span", { class: "secret" });
  for (const c of secret) el.appendChild(h("span", { class: `ch-${charClass(c)}` }, c));
  return el;
}

/** Caixa de erro. / Error box. */
export function errorBox(): { el: HTMLElement; show(msg: string): void; clear(): void } {
  const el = h("div", { class: "error", role: "alert", hidden: true });
  return {
    el,
    show(msg) {
      el.replaceChildren(icon("warning", 16), h("span", null, msg));
      el.hidden = false;
    },
    clear() {
      el.hidden = true;
      el.replaceChildren();
    },
  };
}

/** Converte um erro do Rust em texto traduzido. / Turns a Rust error into translated text. */
export function errorMessage(e: unknown): string {
  if (isCmdError(e)) {
    const key = `err_${e.code}`;
    return hasKey(key) ? t(key, { secs: e.retryAfterSecs ?? 0 }) : t("err_internal");
  }
  console.error(e);
  return t("err_internal");
}

/** Desativa o botão enquanto a ação roda. / Disables the button while the action runs. */
export async function busy<T>(btn: HTMLButtonElement, fn: () => Promise<T>): Promise<T> {
  const original = Array.from(btn.childNodes);
  btn.disabled = true;
  btn.replaceChildren(h("span", { class: "spinner" }), h("span", null, t("working")));
  try {
    return await fn();
  } finally {
    btn.disabled = false;
    btn.replaceChildren(...original);
  }
}

export function toast(message: string): void {
  const host = document.getElementById("toasts");
  if (!host) return;
  const el = h("div", { class: "toast" }, icon("check", 16), h("span", null, message));
  host.appendChild(el);
  window.setTimeout(() => el.classList.add("toast-out"), 3200);
  window.setTimeout(() => el.remove(), 3600);
}

/** Diálogo modal simples. / Simple modal dialog. */
export function modal(title: string, body: Child[], actions: HTMLButtonElement[]): { close(): void } {
  const dialog = h("div", { class: "modal card", role: "dialog", "aria-modal": "true" }, h("h2", { class: "title" }, title), ...body, h("div", { class: "actions" }, ...actions));
  const backdrop = h("div", { class: "backdrop" }, dialog);
  document.body.appendChild(backdrop);
  actions[0]?.focus();
  return { close: () => backdrop.remove() };
}

/** Separador ornamental. / Ornamental divider. */
export function divider(): HTMLElement {
  return h("div", { class: "divider", "aria-hidden": "true" }, h("span", { class: "divider-gem" }));
}
