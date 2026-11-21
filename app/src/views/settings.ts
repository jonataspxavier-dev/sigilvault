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
          const sites = p.sites.map((site) => {
            const revoke = button(t("set_sites_revoke"), {
              variant: "ghost",
              onClick: async () => {
                try {
                  await busy(revoke, () => api.revokeSite(p.id, site));
                  await load();
                } catch (ex) {
                  err.show(errorMessage(ex));
                }
              },
            });
            return h("li", { class: "site" }, h("span", { class: "value" }, site), revoke);
          });
          return h(
            "li",
            { class: "pairing-block" },
            h(
              "div",
              { class: "pairing" },
              h("div", null, h("strong", null, p.browser), h("div", { class: "muted" }, t("set_browsers_since", { date: since }), p.connected ? ` · ${t("set_browsers_connected")}` : "")),
              remove,
            ),
            sites.length ? h("div", { class: "sites" }, h("div", { class: "muted" }, t("set_sites_allowed")), h("ul", { class: "site-list" }, ...sites)) : null,
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

/** Campo que captura uma combinação de teclas. / Field that captures a key combination. */
function hotkeyInput(value: string): { el: HTMLElement; get(): string } {
  const input = h("input", { class: "input input-mono", readOnly: true, value });
  input.addEventListener("keydown", (e) => {
    e.preventDefault();
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    const key = e.code === "Space" ? "Space" : /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : /^F\d{1,2}$/.test(e.code) ? e.code : null;
    if (!key) return;
    const parts = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win", key].filter(Boolean);
    input.value = parts.join("+");
  });
  const off = h("button", { type: "button", class: "link", onClick: () => (input.value = "") }, t("set_hotkey_off"));
  return { el: h("div", { class: "pw-line" }, input, off), get: () => input.value };
}

/** Atalhos globais, com aviso se o Windows recusou (combinação já usada por outro programa).
 *  Global hotkeys, with a warning if Windows refused (combination already used by another program). */
function hotkeysSection(): HTMLElement {
  const s = session.settings!;
  const search = hotkeyInput(s.hotkeySearch);
  const generate = hotkeyInput(s.hotkeyGenerate);
  const err = errorBox();
  const warn = h("div", { class: "stack" });
  const showInUse = async () => {
    const inUse = await api.hotkeysInUse().catch(() => []);
    const settings = session.settings!;
    warn.replaceChildren(
      ...inUse.map((which) => h("p", { class: "error" }, t("set_hotkey_in_use", { combo: which === "search" ? settings.hotkeySearch : settings.hotkeyGenerate }))),
    );
  };
  const save = button(t("save"), { variant: "primary", icon: "seal", type: "submit" });
  const form = h(
    "form",
    { class: "stack" },
    h("p", { class: "muted" }, t("set_hotkeys_intro")),
    field(t("set_hotkey_search"), search.el),
    field(t("set_hotkey_generate"), generate.el),
    warn,
    err.el,
    h("div", { class: "actions" }, save),
  );
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    err.clear();
    try {
      session.settings = await busy(save, () => api.saveSettings({ ...session.settings!, hotkeySearch: search.get(), hotkeyGenerate: generate.get() }));
      toast(t("set_saved"));
      await showInUse();
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  });
  void showInUse();
  return form;
}

/** Registro dos últimos preenchimentos feitos pelos navegadores. / Log of the latest fills made by the browsers. */
function fillLogSection(): HTMLElement {
  const list = h("ol", { class: "fill-log" });
  const err = errorBox();
  void api
    .fillLog()
    .then((entries) => {
      if (entries.length === 0) return list.replaceChildren(h("li", { class: "muted" }, t("set_log_none")));
      const locale = getLang() === "pt" ? "pt-BR" : "en-US";
      list.replaceChildren(
        ...entries.slice(0, 50).map((e) =>
          h("li", null, h("span", { class: "muted" }, new Date(e.at * 1000).toLocaleString(locale)), " · ", h("strong", null, e.title), e.username ? ` (${e.username})` : "", " · ", e.origin, " · ", e.browser),
        ),
      );
    })
    .catch((ex: unknown) => err.show(errorMessage(ex)));
  return h("div", { class: "stack" }, h("p", { class: "muted" }, t("set_log_intro")), list, err.el);
}

export function settingsPanel(username: string, onLanguageChange: () => void): HTMLElement {
  const s = session.settings!;

  // Preferências / Preferences
  const lang = h("select", { class: "input" }, h("option", { value: "pt", selected: s.language === "pt" }, "Português (Brasil)"), h("option", { value: "en", selected: s.language === "en" }, "English"));
  const autolock = h("input", { type: "number", class: "input input-narrow", min: 1, max: 120, value: String(s.autoLockMinutes) });
  const clip = h("input", { type: "number", class: "input input-narrow", min: 10, max: 120, value: String(s.clipboardClearSecs) });
  const startup = h("input", { type: "checkbox", checked: s.startWithWindows });
  const startupField = h("label", { class: "check" }, startup, h("span", null, t("set_startup"), h("span", { class: "field-hint block" }, t("set_startup_hint"))));
  const prefErr = errorBox();
  const prefSave = button(t("save"), { variant: "primary", icon: "seal", type: "submit" });
  const prefForm = h("form", { class: "stack" }, field(t("set_language"), lang), field(t("set_autolock"), autolock), field(t("set_clipboard"), clip), startupField, prefErr.el, h("div", { class: "actions" }, prefSave));
  prefForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    prefErr.clear();
    try {
      const before = session.settings!.language;
      session.settings = await busy(prefSave, () =>
        api.saveSettings({ ...session.settings!, language: lang.value as Lang, autoLockMinutes: Number(autolock.value), clipboardClearSecs: Number(clip.value), startWithWindows: startup.checked }),
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
    h("h3", { class: "subtitle" }, t("set_hotkeys")),
    hotkeysSection(),
    divider(),
    h("h3", { class: "subtitle" }, t("set_browsers")),
    browsersSection(),
    divider(),
    h("h3", { class: "subtitle" }, t("set_log")),
    fillLogSection(),
    divider(),
    h("h3", { class: "subtitle" }, t("set_about")),
    h("p", { class: "muted" }, t("about_text")),
  );
}
