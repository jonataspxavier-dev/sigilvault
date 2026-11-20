// Tela principal do cofre: busca, lista, detalhes e edição de itens.
// Main vault screen: search, list, item details and editing.

import { api, type Item, type ItemInput, type ItemKind } from "../api";
import { h, mount } from "../dom";
import { getLang, t } from "../i18n";
import { icon, type IconName } from "../icons";
import { session } from "../nav";
import { busy, button, coloredSecret, errorBox, errorMessage, field, modal, passwordInput, textInput, toast } from "../ui";
import { formatDate, parseTags } from "../util";
import { generatorPanel } from "./generator";
import { settingsPanel } from "./settings";
import { transferPanel } from "./transfer";

/** Segundos até ocultar uma senha mostrada. / Seconds until a shown password is hidden again. */
const REVEAL_SECS = 30;

type Mode = { kind: "empty" } | { kind: "detail"; id: string } | { kind: "edit"; id: string } | { kind: "new" } | { kind: "generator" } | { kind: "settings" } | { kind: "transfer" };

/** Etiqueta das entradas criadas pelo atalho de gerar (igual a `desktop::PENDING_TAG`). / Tag of generate-hotkey entries. */
const PENDING_TAG = "pendente";
const KIND_ICON: Record<ItemKind, IconName> = { login: "key", secure_note: "scroll", program: "shield" };

function kindLabel(kind: ItemKind): string {
  return t(kind === "login" ? "kind_login" : kind === "program" ? "kind_program" : "kind_secure_note");
}

export function showVault(opts: { rollbackWarning?: boolean } = {}): void {
  const root = document.getElementById("app")!;
  let items: Item[] = [];
  let query = "";
  let mode: Mode = { kind: "empty" };
  let username = "";
  /** Limpa senhas mostradas e cronômetros ao trocar de painel. / Clears shown passwords and timers on panel change. */
  let cleanup: (() => void)[] = [];

  const search = h("input", { type: "search", class: "input search-input", placeholder: t("search_placeholder"), spellcheck: false, autocomplete: "off" });
  const list = h("ul", { class: "item-list", role: "listbox" });
  const panel = h("section", { class: "panel card" });

  let searchTimer: number | undefined;
  search.addEventListener("input", () => {
    window.clearTimeout(searchTimer);
    searchTimer = window.setTimeout(() => {
      query = search.value;
      void refresh();
    }, 120);
  });

  const header = h(
    "header",
    { class: "topbar" },
    h("div", { class: "brand brand-small" }, h("img", { src: "/logo.svg", alt: "", class: "brand-logo" }), h("span", { class: "brand-name" }, "SigilVault")),
    h(
      "nav",
      { class: "topbar-actions" },
      button(t("new_item"), { icon: "plus", variant: "primary", onClick: () => go({ kind: "new" }) }),
      button(t("generator"), { icon: "quill", onClick: () => go({ kind: "generator" }) }),
      button(t("transfer"), { icon: "chest", onClick: () => go({ kind: "transfer" }) }),
      button(t("settings"), { icon: "gear", onClick: () => go({ kind: "settings" }) }),
      button(t("lock"), { icon: "lock", variant: "ghost", onClick: () => void api.lock() }),
    ),
  );

  mount(root, h("div", { class: "vault" }, header, h("div", { class: "vault-body" }, h("aside", { class: "sidebar card" }, h("div", { class: "search" }, icon("search", 16), search), list), panel)));

  function go(next: Mode) {
    for (const fn of cleanup) fn();
    cleanup = [];
    mode = next;
    renderList();
    renderPanel();
  }

  async function refresh() {
    try {
      items = await api.listItems(query);
      const state = await api.appState();
      username = state.username ?? "";
      session.remainingCodes = state.remainingCodes;
    } catch (e) {
      toast(errorMessage(e));
    }
    renderList();
  }

  function renderList() {
    if (items.length === 0) {
      mount(list, h("li", { class: "list-empty muted" }, query ? t("no_results") : t("empty_vault")));
      return;
    }
    const selected = "id" in mode ? mode.id : null;
    mount(
      list,
      ...items.map((item) =>
        h(
          "li",
          {
            class: item.id === selected ? "item selected" : "item",
            role: "option",
            "aria-selected": String(item.id === selected),
            tabIndex: 0,
            onClick: () => go({ kind: "detail", id: item.id }),
            onKeydown: (e: KeyboardEvent) => e.key === "Enter" && go({ kind: "detail", id: item.id }),
          },
          h("span", { class: "item-icon" }, icon(KIND_ICON[item.kind], 20)),
          h("span", { class: "item-text" }, h("span", { class: "item-title" }, item.title), h("span", { class: "item-sub" }, item.username || item.url || kindLabel(item.kind))),
        ),
      ),
    );
  }

  function renderPanel() {
    switch (mode.kind) {
      case "empty":
        return mount(panel, h("div", { class: "panel-empty" }, h("img", { src: "/logo.svg", alt: "", class: "panel-empty-logo" }), h("p", { class: "muted" }, t("select_item"))));
      case "detail":
        return renderDetail(mode.id);
      case "edit": {
        const id = mode.id;
        return renderEditor(items.find((i) => i.id === id) ?? null);
      }
      case "new":
        return renderEditor(null);
      case "generator":
        return mount(panel, h("h2", { class: "title" }, t("gen_title")), generatorPanel());
      case "settings":
        return mount(panel, settingsPanel(username, () => showVault()));
      case "transfer":
        return mount(panel, transferPanel(username, () => void refresh()));
    }
  }

  // ── Detalhes / Details ─────────────────────────────────────────

  function renderDetail(id: string) {
    const item = items.find((i) => i.id === id);
    if (!item) return go({ kind: "empty" });

    const copy = (fieldName: "password" | "username") => async () => {
      try {
        toast(t("copied", { secs: await api.copyField(item.id, fieldName) }));
      } catch (e) {
        toast(errorMessage(e));
      }
    };

    const rows: HTMLElement[] = [];
    if (item.username) rows.push(row(t("field_username"), h("span", { class: "value" }, item.username), button(t("copy"), { icon: "copy", iconOnly: true, onClick: copy("username") })));
    if (item.url) rows.push(row(t("field_url"), h("span", { class: "value" }, item.url)));
    if (item.appPath) rows.push(row(t("field_app_path"), h("div", { class: "value" }, item.appPath, item.appSigned ? h("div", { class: "muted small" }, t("app_signed")) : null)));
    if (item.hasPassword) {
      const value = h("span", { class: "value masked" }, "••••••••••••");
      let timer: number | undefined;
      const hide = () => {
        window.clearTimeout(timer);
        value.replaceChildren("••••••••••••");
        value.classList.add("masked");
        toggle.replaceChildren(icon("eye"));
        shown = false;
      };
      let shown = false;
      const toggle = button(t("show"), {
        icon: "eye",
        iconOnly: true,
        onClick: async () => {
          if (shown) return hide();
          try {
            const pw = await api.revealPassword(item.id);
            if (pw === null) return;
            value.replaceChildren(coloredSecret(pw));
            value.classList.remove("masked");
            toggle.replaceChildren(icon("eyeOff"));
            shown = true;
            timer = window.setTimeout(hide, REVEAL_SECS * 1000);
          } catch (e) {
            toast(errorMessage(e));
          }
        },
      });
      cleanup.push(hide);
      rows.push(row(t("field_password"), value, toggle, button(t("copy"), { icon: "copy", iconOnly: true, onClick: copy("password") })));
    }
    if (item.historyLen > 0) {
      const list = h("ol", { class: "history" });
      const show = h(
        "button",
        {
          type: "button",
          class: "link",
          onClick: async () => {
            try {
              const entries = await api.revealHistory(item.id);
              list.replaceChildren(
                ...entries.map((e) => h("li", null, h("span", { class: "muted" }, new Date(e.changedAt * 1000).toLocaleString(getLang() === "pt" ? "pt-BR" : "en-US")), " ", h("span", { class: "value" }, coloredSecret(e.password)))),
              );
              show.remove();
            } catch (e) {
              toast(errorMessage(e));
            }
          },
        },
        t("show_history", { n: item.historyLen }),
      );
      // Some ao sair da tela. / Disappears when leaving the screen.
      cleanup.push(() => list.replaceChildren());
      rows.push(row(t("field_history"), h("div", { class: "value" }, show, list)));
    }
    if (item.hasNotes) {
      const notes = h("div", { class: "notes" });
      const show = h(
        "button",
        {
          type: "button",
          class: "link",
          onClick: async () => {
            try {
              notes.textContent = (await api.revealNotes(item.id)) ?? "";
              show.remove();
            } catch (e) {
              toast(errorMessage(e));
            }
          },
        },
        t("show_notes"),
      );
      cleanup.push(() => (notes.textContent = ""));
      rows.push(row(t("field_notes"), h("div", { class: "value" }, show, notes)));
    }
    if (item.tags.length) rows.push(row(t("field_tags").split(" (")[0]!, h("span", { class: "tags" }, ...item.tags.map((tag) => h("span", { class: "tag" }, tag)))));

    // Entrada "pendente" criada pelo atalho de gerar: o usuário confirma quando a conta existir.
    // "Pending" entry created by the generate hotkey: the user confirms once the account exists.
    const pending = item.tags.includes(PENDING_TAG)
      ? h(
          "div",
          { class: "pending stack" },
          h("span", null, t("pending_notice")),
          h(
            "div",
            { class: "actions" },
            button(t("pending_confirm"), {
              icon: "check",
              variant: "primary",
              onClick: async () => {
                try {
                  await api.updateItem(item.id, {
                    kind: item.kind,
                    title: item.title,
                    username: item.username,
                    url: item.url,
                    tags: item.tags.filter((tag) => tag !== PENDING_TAG),
                    matchSubdomains: item.matchSubdomains,
                    appPath: item.appPath,
                  });
                  await refresh();
                  go({ kind: "detail", id: item.id });
                } catch (e) {
                  toast(errorMessage(e));
                }
              },
            }),
          ),
        )
      : null;

    mount(
      panel,
      h("div", { class: "detail-head" }, h("span", { class: "detail-icon" }, icon(KIND_ICON[item.kind], 28)), h("div", null, h("h2", { class: "title" }, item.title), h("div", { class: "muted small" }, kindLabel(item.kind), " · ", t("updated_at", { date: formatDate(item.updatedAt, getLang()) })))),
      pending,
      h("dl", { class: "rows" }, ...rows),
      h(
        "div",
        { class: "actions" },
        button(t("delete"), { icon: "trash", variant: "danger", onClick: () => confirmDelete(item) }),
        button(t("edit"), { icon: "edit", variant: "primary", onClick: () => go({ kind: "edit", id: item.id }) }),
      ),
    );
  }

  function row(label: string, ...content: HTMLElement[]): HTMLElement {
    return h("div", { class: "row" }, h("dt", null, label), h("dd", null, ...content));
  }

  function confirmDelete(item: Item) {
    const dlg = modal(t("delete"), [h("p", null, t("delete_confirm", { title: item.title }))], [
      button(t("cancel"), { onClick: () => dlg.close() }),
      button(t("delete"), {
        icon: "trash",
        variant: "danger",
        onClick: async () => {
          dlg.close();
          try {
            await api.deleteItem(item.id);
            toast(t("deleted"));
            await refresh();
            go({ kind: "empty" });
          } catch (e) {
            toast(errorMessage(e));
          }
        },
      }),
    ]);
  }

  // ── Editor ─────────────────────────────────────────────────────

  function renderEditor(existing: Item | null) {
    const kind = h(
      "select",
      { class: "input" },
      h("option", { value: "login" }, t("kind_login")),
      h("option", { value: "program" }, t("kind_program")),
      h("option", { value: "secure_note" }, t("kind_secure_note")),
    );
    kind.value = existing?.kind ?? "login";
    const title = textInput({ value: existing?.title, maxLength: 200 });
    const user = textInput({ value: existing?.username, autocomplete: "off", maxLength: 300 });
    const url = textInput({ value: existing?.url, placeholder: "https://", maxLength: 2000 });
    const subdomains = h("input", { type: "checkbox", checked: existing?.matchSubdomains ?? false });
    const appPath = textInput({ value: existing?.appPath, placeholder: "C:\\Program Files\\…\\programa.exe", maxLength: 1000 });
    const pw = passwordInput({ placeholder: existing?.hasPassword ? t("password_keep") : "", autocomplete: "new-password" });
    const removePw = h("input", { type: "checkbox" });
    const notes = h("textarea", { class: "input textarea", rows: 5, spellcheck: false, maxLength: 20000 });
    const tags = textInput({ value: existing?.tags.join(", "), maxLength: 500 });
    const err = errorBox();
    const save = button(t("save"), { icon: "seal", variant: "primary", type: "submit" });

    // Notas existentes só são carregadas se o usuário pedir. / Existing notes load only on request.
    let notesLoaded = !existing?.hasNotes;
    const notesField = h("div", null);
    if (notesLoaded) {
      notesField.appendChild(notes);
    } else {
      const load = h(
        "button",
        {
          type: "button",
          class: "link",
          onClick: async () => {
            try {
              notes.value = (await api.revealNotes(existing!.id)) ?? "";
              notesLoaded = true;
              notesField.replaceChildren(notes);
            } catch (e) {
              err.show(errorMessage(e));
            }
          },
        },
        t("notes_load"),
      );
      notesField.appendChild(load);
    }

    const genHost = h("div", { class: "gen-host" });
    const genBtn = button(t("generate"), {
      icon: "quill",
      onClick: () => {
        if (genHost.childElementCount) return genHost.replaceChildren();
        genHost.replaceChildren(
          generatorPanel({
            compact: true,
            onUse: (p) => {
              pw.input.value = p;
              removePw.checked = false;
              genHost.replaceChildren();
            },
          }),
        );
      },
    });

    const siteOnly = [field(t("field_url"), url), h("label", { class: "check small" }, subdomains, h("span", null, t("match_subdomains")))];
    const programOnly = [field(t("field_app_path"), appPath, t("app_path_hint"))];
    const loginOnly = [field(t("field_username"), user), ...siteOnly, ...programOnly, h("div", { class: "field" }, h("span", { class: "field-label" }, t("field_password")), h("div", { class: "pw-line" }, pw.wrap, genBtn), existing?.hasPassword ? h("label", { class: "check small" }, removePw, h("span", null, t("remove_password"))) : null, genHost)];
    const syncKind = () => {
      // Login e Programa têm usuário e senha; o endereço é do login, o caminho é do programa.
      // Login and Program have username and password; the address belongs to login, the path to program.
      loginOnly.forEach((el) => (el.hidden = kind.value === "secure_note"));
      siteOnly.forEach((el) => (el.hidden = kind.value !== "login"));
      programOnly.forEach((el) => (el.hidden = kind.value !== "program"));
    };
    kind.addEventListener("change", syncKind);
    syncKind();

    const form = h(
      "form",
      { class: "stack", novalidate: true },
      h("h2", { class: "title" }, existing ? t("edit_title_edit") : t("edit_title_new")),
      field(t("field_kind"), kind),
      field(t("field_title"), title),
      ...loginOnly,
      h("div", { class: "field" }, h("span", { class: "field-label" }, t("field_notes")), notesField),
      field(t("field_tags"), tags),
      err.el,
      h("div", { class: "actions" }, button(t("cancel"), { variant: "ghost", onClick: () => go(existing ? { kind: "detail", id: existing.id } : { kind: "empty" }) }), save),
    );

    form.addEventListener("submit", async (e) => {
      e.preventDefault();
      err.clear();
      if (!title.value.trim()) return err.show(t("required"));
      const isLogin = kind.value !== "secure_note";
      const input: ItemInput = {
        kind: kind.value as ItemKind,
        title: title.value.trim(),
        username: isLogin ? user.value.trim() : "",
        url: kind.value === "login" ? url.value.trim() : "",
        tags: parseTags(tags.value),
        matchSubdomains: kind.value === "login" && subdomains.checked,
        appPath: kind.value === "program" ? appPath.value.trim() : "",
      };
      if (existing) {
        if (!isLogin || removePw.checked) input.password = existing.hasPassword ? "" : undefined;
        else if (pw.input.value) input.password = pw.input.value;
        if (notesLoaded) input.notes = notes.value;
      } else {
        if (isLogin && pw.input.value) input.password = pw.input.value;
        if (notes.value) input.notes = notes.value;
      }
      try {
        const id = await busy(save, async () => {
          if (existing) {
            await api.updateItem(existing.id, input);
            return existing.id;
          }
          return api.addItem(input);
        });
        pw.input.value = "";
        notes.value = "";
        toast(t("saved"));
        await refresh();
        go({ kind: "detail", id });
      } catch (ex) {
        err.show(errorMessage(ex));
      }
    });

    cleanup.push(() => {
      pw.input.value = "";
      notes.value = "";
    });
    mount(panel, form);
    title.focus();
  }

  // Atalhos: Ctrl+F busca, Esc volta. / Shortcuts: Ctrl+F search, Esc back.
  document.onkeydown = (e) => {
    if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      search.focus();
    } else if (e.key === "Escape" && !document.querySelector(".backdrop")) {
      if (mode.kind === "edit") go({ kind: "detail", id: mode.id });
      else if (mode.kind !== "empty") go({ kind: "empty" });
    }
  };

  void refresh().then(renderPanel);

  // Cofre sem códigos (ex.: cópia com senha própria): avisa e aponta para Configurações.
  // Vault without codes (e.g. copy with its own password): warn and point to Settings.
  if (session.remainingCodes === 0) {
    const dlg = modal(t("zero_codes_title"), [h("p", null, t("zero_codes_text"))], [
      button(t("ok"), { onClick: () => dlg.close() }),
      button(t("settings"), { icon: "gear", variant: "primary", onClick: () => (dlg.close(), go({ kind: "settings" })) }),
    ]);
  }

  if (opts.rollbackWarning) {
    const dlg = modal(t("rollback_title"), [h("p", null, t("rollback_text"))], [button(t("ok"), { variant: "primary", onClick: () => dlg.close() })]);
  }
}
