// Pop-up rápido (atalho global): buscar e digitar (Auto-Type) ou copiar; ou gerar/só salvar para
// um programa. Funciona inteiro pelo teclado: digite, ↑/↓, Enter digita, Esc fecha.
// Quick popup (global hotkey): search and type (Auto-Type) or copy; or generate/just save for a
// program. Fully keyboard-driven: type, ↑/↓, Enter types, Esc closes.

import "./styles.css";

import { type QuickContext, type QuickItem, api, isCmdError } from "./api";
import { h, mount } from "./dom";
import { setLang, t } from "./i18n";
import { session } from "./nav";
import { busy, button, errorBox, errorMessage, field, passwordInput, textInput, toast } from "./ui";
import { generatorPanel } from "./views/generator";

const root = document.getElementById("app")!;
let ctx: QuickContext;

function close(): void {
  void api.quickClose();
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") close();
});

/** Cabeçalho: para onde vai digitar, e por que não dá (se for o caso).
 *  Header: where it will type, and why it cannot (if so). */
function header(): HTMLElement {
  const target = ctx.target;
  const where = target
    ? h("div", { class: "quick-target" }, h("span", { class: "muted" }, t("quick_typing_into")), " ", h("strong", null, target.exeName), target.title ? h("span", { class: "muted" }, ` — ${target.title}`) : null)
    : h("div", { class: "quick-target muted" }, t("quick_no_target"));
  const warn = !target ? null : target.browser ? t("quick_browser") : target.elevated ? t("quick_elevated") : null;
  return h(
    "header",
    { class: "quick-head" },
    h("div", { class: "brand" }, h("img", { src: "/logo.svg", alt: "", class: "brand-logo quick-logo" }), h("div", { class: "brand-name" }, "SigilVault")),
    where,
    warn ? h("p", { class: "error" }, warn) : null,
  );
}

/** Dá para digitar no alvo? / Can it type into the target? */
function canType(): boolean {
  return !!ctx.target && !ctx.target.browser && !ctx.target.elevated;
}

// ── Desbloquear / Unlock ──────────────────────────────────────────

function showUnlock(): void {
  const pw = passwordInput({ autocomplete: "current-password" });
  const err = errorBox();
  const submit = button(t("unlock_button"), { variant: "primary", type: "submit", icon: "lock" });
  const form = h("form", { class: "stack" }, field(t("master_password"), pw.wrap), err.el, h("div", { class: "actions" }, button(t("cancel"), { variant: "ghost", onClick: close }), submit));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!pw.input.value) return;
    try {
      await busy(submit, () => api.unlock(pw.input.value));
      pw.input.value = "";
      await start();
    } catch (ex) {
      pw.input.select();
      err.show(isCmdError(ex) && ex.code === "locked_out" ? t("err_locked_out", { secs: ex.retryAfterSecs ?? 0 }) : errorMessage(ex));
    }
  });
  mount(root, h("section", { class: "quick card" }, header(), h("p", null, t("quick_locked")), form));
  pw.input.focus();
}

// ── Buscar / Search ───────────────────────────────────────────────

function showSearch(): void {
  const query = textInput({ placeholder: t("quick_search_placeholder"), autocomplete: "off", maxLength: 100 });
  const list = h("ul", { class: "quick-list", role: "listbox" });
  const err = errorBox();
  let items: QuickItem[] = [];
  let selected = 0;

  const act = async (item: QuickItem, how: "type" | "password" | "username") => {
    err.clear();
    try {
      if (how === "type") {
        // O pop-up some, a senha é digitada no programa e ele fecha. / The popup hides, the password is typed and it closes.
        await api.quickType(item.id);
      } else {
        const secs = await api.copyField(item.id, how);
        toast(t("copied", { secs }));
        setTimeout(close, 900);
      }
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  };

  const render = () => {
    list.replaceChildren(
      ...items.map((item, i) =>
        h(
          "li",
          { class: `quick-item${i === selected ? " selected" : ""}`, role: "option", "aria-selected": i === selected },
          h(
            "div",
            { class: "quick-item-text" },
            h("strong", null, item.title),
            h("span", { class: "muted" }, item.username || "—"),
            item.matched ? h("span", { class: "quick-badge" }, t("quick_this_program")) : null,
          ),
          h(
            "div",
            { class: "quick-item-actions" },
            canType() ? button(t("quick_type"), { icon: "key", variant: i === selected ? "primary" : "secondary", onClick: () => void act(item, "type") }) : null,
            button(t("quick_copy_password"), { icon: "copy", iconOnly: true, onClick: () => void act(item, "password") }),
            item.username ? button(t("quick_copy_username"), { icon: "user", iconOnly: true, onClick: () => void act(item, "username") }) : null,
          ),
        ),
      ),
    );
    if (items.length === 0) list.appendChild(h("li", { class: "muted quick-empty" }, t("quick_none")));
    list.querySelector(".selected")?.scrollIntoView({ block: "nearest" });
  };

  let seq = 0;
  const search = async () => {
    const mine = ++seq;
    try {
      const found = await api.quickSearch(query.value);
      if (mine !== seq) return;
      items = found;
      selected = 0;
      render();
    } catch (ex) {
      err.show(errorMessage(ex));
    }
  };

  query.addEventListener("input", () => void search());
  query.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length) selected = (selected + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length;
      render();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selected];
      // Enter digita; se não dá para digitar aqui, copia a senha. / Enter types; if typing is not possible here, copies the password.
      if (item) void act(item, canType() ? "type" : "password");
    }
  });

  mount(root, h("section", { class: "quick card" }, header(), query, list, err.el, h("p", { class: "quick-hint muted" }, canType() ? t("quick_hint_type") : t("quick_hint_copy"))));
  query.focus();
  void search();
}

// ── Gerar ou só salvar (programa) / Generate or just save (program) ─

function showGenerate(which: "generate" | "save"): void {
  const target = ctx.target;
  if (!target || target.browser) {
    mount(root, h("section", { class: "quick card" }, header(), h("p", { class: "muted" }, target?.browser ? t("quick_browser") : t("quick_no_target")), h("div", { class: "actions" }, button(t("cancel"), { variant: "ghost", onClick: close }))));
    return;
  }
  const title = textInput({ value: target.exeName.replace(/\.exe$/i, ""), maxLength: 200 });
  const user = textInput({ autocomplete: "off", maxLength: 300 });
  const err = errorBox();
  const tab = (id: "generate" | "save", label: string) =>
    h("button", { type: "button", class: `tab${which === id ? " active" : ""}`, onClick: () => showGenerate(id) }, label);
  const tabs = h("nav", { class: "quick-tabs" }, tab("generate", t("quick_tab_generate")), tab("save", t("quick_tab_save")));
  const common = [field(t("field_title"), title), field(t("quick_username_optional"), user)];

  if (which === "generate") {
    const panel = generatorPanel({
      compact: true,
      minimal: true,
      useLabel: canType() ? t("quick_use_and_save") : t("quick_save_only"),
      onUse: async (password) => {
        err.clear();
        try {
          // Salva como "pendente" ANTES de digitar. / Saves as "pending" BEFORE typing.
          await api.quickSaveProgram(title.value, user.value, password, canType());
          if (!canType()) close();
        } catch (ex) {
          err.show(errorMessage(ex));
        }
      },
    });
    mount(root, h("section", { class: "quick card" }, header(), tabs, ...common, panel, h("p", { class: "muted quick-hint" }, t("quick_pending_note")), err.el));
  } else {
    const pw = passwordInput({ autocomplete: "off" });
    const save = button(t("save"), { variant: "primary", icon: "seal", type: "submit" });
    const form = h("form", { class: "stack" }, ...common, field(t("field_password"), pw.wrap), err.el, h("div", { class: "actions" }, button(t("cancel"), { variant: "ghost", onClick: close }), save));
    form.addEventListener("submit", async (e) => {
      e.preventDefault();
      if (!pw.input.value) return err.show(t("required"));
      try {
        await busy(save, () => api.quickSaveProgram(title.value, user.value, pw.input.value, false));
      } catch (ex) {
        err.show(errorMessage(ex));
      }
    });
    mount(root, h("section", { class: "quick card" }, header(), tabs, form));
  }
  title.focus();
}

// ── Início / Start ────────────────────────────────────────────────

async function start(): Promise<void> {
  ctx = await api.quickContext();
  setLang(ctx.language);
  // O gerador lê as preferências daqui. / The generator reads preferences from here.
  session.settings = { language: ctx.language, autoLockMinutes: 5, clipboardClearSecs: 30, generator: ctx.generator, hotkeySearch: "", hotkeyGenerate: "", startWithWindows: false };
  if (!ctx.unlocked) return showUnlock();
  if (ctx.mode === "generate") showGenerate("generate");
  else showSearch();
}

start().catch((e) => {
  root.textContent = errorMessage(e);
});
