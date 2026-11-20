// Telas de entrada: criar cofre, códigos de recuperação, desbloquear e recuperar.
// Entry screens: create vault, recovery codes, unlock and recover.

import { api, isCmdError, type AppState } from "../api";
import { h, mount } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";
import { nav, session } from "../nav";
import { busy, button, divider, errorBox, errorMessage, field, modal, passwordInput, strengthMeter, textInput } from "../ui";
import { formatCodeInput } from "../util";

function root(): HTMLElement {
  return document.getElementById("app")!;
}

/** Cartão central com o logo. / Centered card with the logo. */
function authCard(title: string, ...content: (Node | null)[]): HTMLElement {
  return h(
    "section",
    { class: "auth" },
    h(
      "div",
      { class: "card auth-card" },
      h("div", { class: "brand" }, h("img", { src: "/logo.svg", alt: "", class: "brand-logo" }), h("div", null, h("div", { class: "brand-name" }, "SigilVault"), h("div", { class: "brand-tagline" }, t("tagline")))),
      divider(),
      h("h1", { class: "title" }, title),
      ...content,
    ),
  );
}

/** Formulário de senha nova com confirmação e medidor. / New password form with confirmation and meter. */
function newPasswordFields(label: string, getUsername: () => string) {
  const pw = passwordInput({ autocomplete: "new-password" });
  const confirm = passwordInput({ autocomplete: "new-password" });
  const meter = strengthMeter(getUsername);
  meter.watch(pw.input);
  const valid = () => meter.score() >= 3 && pw.input.value === confirm.input.value;
  return {
    pw,
    confirm,
    meter,
    nodes: [field(label, pw.wrap, t("master_hint")), meter.el, field(t("confirm_password"), confirm.wrap)],
    /** Mensagem do problema, ou null. / Problem message, or null. */
    problem(): string | null {
      if (meter.score() < 3) return t("strength_required");
      if (pw.input.value !== confirm.input.value) return t("passwords_dont_match");
      return null;
    },
    valid,
  };
}

// ── Criar / Create ────────────────────────────────────────────────

export function showCreate(): void {
  const username = textInput({ autocomplete: "username", maxLength: 64 });
  const fields = newPasswordFields(t("master_password"), () => username.value);
  const err = errorBox();
  const submit = button(t("create_button"), { variant: "primary", type: "submit", icon: "seal" });

  const form = h("form", { class: "stack", novalidate: true }, field(t("username"), username, t("username_hint")), ...fields.nodes, err.el, h("div", { class: "actions" }, submit));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    err.clear();
    if (!username.value.trim()) return err.show(t("required"));
    const problem = fields.problem();
    if (problem) return err.show(problem);
    try {
      const codes = await busy(submit, () => api.createVault(username.value.trim(), fields.pw.input.value));
      fields.pw.input.value = "";
      fields.confirm.input.value = "";
      showCodes(codes, t("codes_title"), () => nav().vault());
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  });

  const importErr = errorBox();
  const importLink = h(
    "button",
    {
      type: "button",
      class: "link",
      onClick: async () => {
        importErr.clear();
        try {
          if (await api.importVaultFile()) await nav().boot();
        } catch (ex) {
          importErr.show(errorMessage(ex));
        }
      },
    },
    t("import_vault_link"),
  );

  mount(root(), authCard(t("create_title"), h("p", { class: "muted" }, t("create_intro")), form, divider(), importLink, importErr.el));
  username.focus();
}

// ── Códigos / Codes ───────────────────────────────────────────────

/** Mostra os códigos uma única vez. Ao continuar, eles são removidos da tela.
 *  Shows the codes exactly once. On continue, they are removed from the screen. */
export function showCodes(codes: string[], title: string, onDone: () => void): void {
  const list = h("ol", { class: "codes print-area" }, ...codes.map((c) => h("li", { class: "code" }, c)));
  const check = h("input", { type: "checkbox" });
  const done = button(t("continue"), { variant: "primary", icon: "check" });
  done.disabled = true;
  check.addEventListener("change", () => (done.disabled = !check.checked));
  done.addEventListener("click", () => {
    list.replaceChildren();
    codes.fill("");
    onDone();
  });
  const warn = (text: string) => h("li", null, icon("warning", 16), h("span", null, text));

  mount(
    root(),
    authCard(
      title,
      h("p", { class: "muted" }, t("codes_intro")),
      list,
      h("ul", { class: "warnings" }, warn(t("codes_warn_power")), warn(t("codes_warn_store")), warn(t("codes_warn_once")), warn(t("codes_warn_noserver"))),
      h("label", { class: "check" }, check, h("span", null, t("codes_confirm"))),
      h("div", { class: "actions" }, button(t("print"), { icon: "print", onClick: () => window.print() }), done),
    ),
  );
}

// ── Desbloquear / Unlock ──────────────────────────────────────────

export function showUnlock(state: AppState): void {
  const pw = passwordInput({ autocomplete: "current-password" });
  const err = errorBox();
  const submit = button(t("unlock_button"), { variant: "primary", type: "submit", icon: "lock" });
  let countdown: number | undefined;

  const startCountdown = (secs: number) => {
    window.clearInterval(countdown);
    let left = secs;
    submit.disabled = true;
    const tick = () => {
      if (left <= 0) {
        window.clearInterval(countdown);
        submit.disabled = false;
        err.clear();
        return;
      }
      err.show(t("locked_out", { secs: left }));
      left -= 1;
    };
    tick();
    countdown = window.setInterval(tick, 1000);
  };

  const form = h("form", { class: "stack", novalidate: true }, field(t("master_password"), pw.wrap), err.el, h("div", { class: "actions" }, submit));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!pw.input.value) return;
    err.clear();
    try {
      const res = await busy(submit, () => api.unlock(pw.input.value));
      pw.input.value = "";
      session.remainingCodes = res.remainingCodes;
      nav().vault({ rollbackWarning: res.rollbackWarning });
    } catch (ex) {
      pw.input.select();
      if (isCmdError(ex) && ex.code === "locked_out" && ex.retryAfterSecs) startCountdown(ex.retryAfterSecs);
      else err.show(errorMessage(ex));
    }
  });

  const forgot = h("button", { type: "button", class: "link", onClick: () => nav().recover() }, t("forgot"));
  mount(root(), authCard(t("unlock_title"), state.username ? h("p", { class: "welcome" }, t("welcome_back", { name: state.username })) : null, form, forgot));
  if (state.retryAfterSecs > 0) startCountdown(state.retryAfterSecs);
  pw.input.focus();
}

// ── Recuperar / Recover ───────────────────────────────────────────

export function showRecover(): void {
  const code = textInput({ placeholder: "XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXX", maxLength: 40 });
  code.classList.add("input-mono");
  code.addEventListener("input", () => (code.value = formatCodeInput(code.value)));
  const fields = newPasswordFields(t("new_master_password"), () => "");
  const err = errorBox();
  const submit = button(t("recover_button"), { variant: "primary", type: "submit", icon: "key" });

  const form = h("form", { class: "stack", novalidate: true }, field(t("recovery_code"), code), ...fields.nodes, err.el, h("div", { class: "actions" }, button(t("back"), { icon: "back", variant: "ghost", onClick: () => void nav().boot() }), submit));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    err.clear();
    if (!code.value) return err.show(t("required"));
    const problem = fields.problem();
    if (problem) return err.show(problem);
    try {
      const res = await busy(submit, () => api.recover(code.value, fields.pw.input.value));
      code.value = "";
      fields.pw.input.value = "";
      fields.confirm.input.value = "";
      session.remainingCodes = res.remainingCodes;
      const dlg = modal(
        t("recovered_title"),
        [
          h("p", { class: "welcome" }, t("recovered_user", { name: res.username })),
          h("p", null, t("recovered_left", { n: res.remainingCodes })),
          h("p", { class: "muted" }, t("recovered_copies")),
        ],
        [button(t("continue"), { variant: "primary", onClick: () => (dlg.close(), nav().vault()) })],
      );
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  });

  mount(root(), authCard(t("recover_title"), h("p", { class: "muted" }, t("recover_intro")), form));
  code.focus();
}
