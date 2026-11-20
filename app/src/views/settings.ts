// Painel de configurações: preferências, troca da senha mestra e códigos de recuperação.
// Settings panel: preferences, master password change and recovery codes.

import { api, type Lang } from "../api";
import { h } from "../dom";
import { getLang, setLang, t } from "../i18n";
import { nav, session } from "../nav";
import { busy, button, divider, errorBox, errorMessage, field, passwordInput, strengthMeter, toast } from "../ui";
import { showCodes } from "./auth";

/** Lista dos navegadores pareados, com botão de remover. / List of paired browsers, with a remove button. */
function browsersSection(): HTMLElement {
  const list = h("ul", { class: "pairings" });
  const err = errorBox();
  const load = async () => {
    err.clear();
    try {
      const entries = await api.listPairings();
      if (entries.length === 0) {
        list.replaceChildren(h("li", { class: "muted" }, t("set_browsers_none")));
        return;
      }
      list.replaceChildren(
        ...entries.map((p) => {
          const since = new Date(p.createdAt * 1000).toLocaleDateString(getLang() === "pt" ? "pt-BR" : "en-US");
          const remove = button(t("set_browsers_remove"), {
            variant: "danger",
            onClick: async () => {
              try {
                await busy(remove, () => api.removePairing(p.id));
                toast(t("set_browsers_removed"));
                await load();
              } catch (ex) {
                err.show(errorMessage(ex));
              }
            },
          });
          return h(
            "li",
            { class: "pairing" },
            h("div", null, h("strong", null, p.browser), h("div", { class: "muted" }, t("set_browsers_since", { date: since }), p.connected ? ` · ${t("set_browsers_connected")}` : "")),
            remove,
          );
        }),
      );
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  };
  void load();
  return h("div", { class: "stack" }, h("p", { class: "muted" }, t("set_browsers_intro")), list, err.el);
}

export function settingsPanel(username: string, onLanguageChange: () => void): HTMLElement {
  const s = session.settings!;

  // Preferências / Preferences
  const lang = h("select", { class: "input" }, h("option", { value: "pt", selected: s.language === "pt" }, "Português (Brasil)"), h("option", { value: "en", selected: s.language === "en" }, "English"));
  const autolock = h("input", { type: "number", class: "input input-narrow", min: 1, max: 120, value: String(s.autoLockMinutes) });
  const clip = h("input", { type: "number", class: "input input-narrow", min: 10, max: 120, value: String(s.clipboardClearSecs) });
  const prefErr = errorBox();
  const prefSave = button(t("save"), { variant: "primary", icon: "seal", type: "submit" });
  const prefForm = h("form", { class: "stack" }, field(t("set_language"), lang), field(t("set_autolock"), autolock), field(t("set_clipboard"), clip), prefErr.el, h("div", { class: "actions" }, prefSave));
  prefForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    prefErr.clear();
    try {
      const before = session.settings!.language;
      session.settings = await busy(prefSave, () =>
        api.saveSettings({ ...session.settings!, language: lang.value as Lang, autoLockMinutes: Number(autolock.value), clipboardClearSecs: Number(clip.value) }),
      );
      setLang(session.settings.language);
      toast(t("set_saved"));
      if (before !== session.settings.language) onLanguageChange();
    } catch (ex) {
      prefErr.show(errorMessage(ex));
    }
  });

  // Trocar senha mestra / Change master password
  const current = passwordInput({ autocomplete: "current-password" });
  const next = passwordInput({ autocomplete: "new-password" });
  const confirm = passwordInput({ autocomplete: "new-password" });
  const meter = strengthMeter(() => username);
  meter.watch(next.input);
  const pwErr = errorBox();
  const pwSave = button(t("change_pw_button"), { variant: "primary", icon: "lock", type: "submit" });
  const pwForm = h("form", { class: "stack" }, field(t("current_password"), current.wrap), field(t("new_master_password"), next.wrap, t("master_hint")), meter.el, field(t("confirm_password"), confirm.wrap), pwErr.el, h("div", { class: "actions" }, pwSave));
  pwForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    pwErr.clear();
    if (meter.score() < 3) return pwErr.show(t("strength_required"));
    if (next.input.value !== confirm.input.value) return pwErr.show(t("passwords_dont_match"));
    try {
      await busy(pwSave, () => api.changeMasterPassword(current.input.value, next.input.value));
      for (const f of [current, next, confirm]) f.input.value = "";
      toast(t("pw_changed"));
    } catch (ex) {
      pwErr.show(errorMessage(ex));
    }
  });

  // Códigos / Codes
  const left = session.remainingCodes ?? 0;
  const regenPw = passwordInput({ autocomplete: "current-password" });
  const regenErr = errorBox();
  const regenBtn = button(t("regen_button"), { icon: "refresh", type: "submit" });
  const regenForm = h(
    "form",
    { class: "stack" },
    h("p", { class: left <= 3 ? "warn-text" : "" }, t("codes_left", { n: left }), left <= 3 ? ` ${t("codes_low")}` : ""),
    h("p", { class: "muted" }, t("regen_intro")),
    field(t("master_password"), regenPw.wrap),
    regenErr.el,
    h("div", { class: "actions" }, regenBtn),
  );
  regenForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    regenErr.clear();
    try {
      const codes = await busy(regenBtn, () => api.regenerateCodes(regenPw.input.value));
      regenPw.input.value = "";
      session.remainingCodes = codes.length;
      showCodes(codes, t("codes_new_title"), () => nav().vault());
    } catch (ex) {
      regenErr.show(errorMessage(ex));
    }
  });

  return h(
    "div",
    { class: "settings" },
    h("h2", { class: "title" }, t("settings")),
    h("h3", { class: "subtitle" }, t("set_preferences")),
    prefForm,
    divider(),
    h("h3", { class: "subtitle" }, t("set_change_pw")),
    pwForm,
    divider(),
    h("h3", { class: "subtitle" }, t("set_codes")),
    regenForm,
    divider(),
    h("h3", { class: "subtitle" }, t("set_browsers")),
    browsersSection(),
    divider(),
    h("h3", { class: "subtitle" }, t("set_about")),
    h("p", { class: "muted" }, t("about_text")),
  );
}
