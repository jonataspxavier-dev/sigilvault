// Painel de exportação, importação e backups. Os arquivos são escolhidos em diálogos nativos
// abertos pelo Rust; aqui só pedimos a ação e mostramos o resultado.
// Export, import and backups panel. Files are picked in native dialogs opened by Rust; here we
// only request the action and show the result.

import { api, type ImportResult } from "../api";
import { h } from "../dom";
import { getLang, t } from "../i18n";
import { icon } from "../icons";
import { busy, button, divider, errorBox, errorMessage, field, passwordInput, strengthMeter, toast } from "../ui";
import { formatDate } from "../util";

export function transferPanel(username: string, onChanged: () => void): HTMLElement {
  return h(
    "div",
    { class: "settings" },
    h("h2", { class: "title" }, t("transfer")),
    exportSection(username),
    divider(),
    importSection(onChanged),
    divider(),
    mergeSection(onChanged),
    divider(),
    backupSection(onChanged),
    divider(),
    csvSection(),
  );
}

// ── Exportar / Export ─────────────────────────────────────────────

function exportSection(username: string): HTMLElement {
  const copy = h("input", { type: "radio", name: "exp-mode", checked: true });
  const withPw = h("input", { type: "radio", name: "exp-mode" });
  const pw = passwordInput({ autocomplete: "new-password" });
  const confirm = passwordInput({ autocomplete: "new-password" });
  const meter = strengthMeter(() => username);
  meter.watch(pw.input);
  const pwFields = h("div", { class: "stack indent", hidden: true }, field(t("exp_password_label"), pw.wrap, t("master_hint")), meter.el, field(t("confirm_password"), confirm.wrap));
  const sync = () => (pwFields.hidden = !withPw.checked);
  copy.addEventListener("change", sync);
  withPw.addEventListener("change", sync);

  const err = errorBox();
  const go = button(t("exp_button"), { icon: "chest", variant: "primary" });
  go.addEventListener("click", async () => {
    err.clear();
    if (withPw.checked) {
      if (meter.score() < 3) return err.show(t("strength_required"));
      if (pw.input.value !== confirm.input.value) return err.show(t("passwords_dont_match"));
    }
    try {
      const done = await busy(go, () => (withPw.checked ? api.exportVault("password", pw.input.value) : api.exportVault("copy")));
      if (done) {
        pw.input.value = "";
        confirm.input.value = "";
        toast(t("exp_done"));
      }
    } catch (e) {
      err.show(errorMessage(e));
    }
  });

  return h(
    "section",
    { class: "stack" },
    h("h3", { class: "subtitle" }, t("exp_title")),
    h("label", { class: "check radio" }, copy, h("span", null, h("strong", null, t("exp_copy")), h("span", { class: "field-hint block" }, t("exp_copy_hint")))),
    h("label", { class: "check radio" }, withPw, h("span", null, h("strong", null, t("exp_password")), h("span", { class: "field-hint block" }, t("exp_password_hint")))),
    pwFields,
    h("p", { class: "muted small" }, t("exp_cloud_ok")),
    err.el,
    h("div", { class: "actions" }, go),
  );
}

// ── Importar / Import ─────────────────────────────────────────────

function resultBox(): { el: HTMLElement; show(r: ImportResult): void } {
  const el = h("div", { class: "result", hidden: true });
  return {
    el,
    show(r) {
      const parts: HTMLElement[] = [h("p", null, icon("check", 16), " ", t("imp_result", { format: r.format, added: r.added, duplicates: r.duplicates, skipped: r.skipped }))];
      if (r.plaintext) {
        const del = button(t("imp_delete"), { icon: "trash", variant: "danger" });
        del.addEventListener("click", async () => {
          try {
            if (await api.deleteImportedFile()) {
              toast(t("imp_deleted"));
              warn.remove();
            }
          } catch (e) {
            toast(errorMessage(e));
          }
        });
        const warn = h("div", { class: "warn-box" }, h("p", null, icon("warning", 16), " ", t("imp_plaintext")), del);
        parts.push(warn);
      }
      el.replaceChildren(...parts);
      el.hidden = false;
    },
  };
}

function importSection(onChanged: () => void): HTMLElement {
  const err = errorBox();
  const result = resultBox();
  const go = button(t("imp_button"), { icon: "chest" });
  go.addEventListener("click", async () => {
    err.clear();
    try {
      const r = await busy(go, () => api.importFile());
      if (r) {
        result.show(r);
        onChanged();
      }
    } catch (e) {
      err.show(errorMessage(e));
    }
  });
  return h("section", { class: "stack" }, h("h3", { class: "subtitle" }, t("imp_title")), h("p", { class: "muted" }, t("imp_hint")), err.el, h("div", { class: "actions" }, go), result.el);
}

function mergeSection(onChanged: () => void): HTMLElement {
  const pw = passwordInput({ autocomplete: "off" });
  const err = errorBox();
  const result = resultBox();
  const go = button(t("merge_button"), { icon: "chest" });
  go.addEventListener("click", async () => {
    err.clear();
    if (!pw.input.value) return err.show(t("required"));
    try {
      const r = await busy(go, () => api.mergeVault(pw.input.value));
      if (r) {
        pw.input.value = "";
        result.show(r);
        onChanged();
      }
    } catch (e) {
      err.show(errorMessage(e));
    }
  });
  return h("section", { class: "stack" }, h("h3", { class: "subtitle" }, t("merge_title")), h("p", { class: "muted" }, t("merge_hint")), field(t("merge_password"), pw.wrap), err.el, h("div", { class: "actions" }, go), result.el);
}

// ── Backups ───────────────────────────────────────────────────────

function backupSection(onChanged: () => void): HTMLElement {
  const list = h("ul", { class: "backup-list" });
  const err = errorBox();

  async function load() {
    try {
      const entries = await api.listBackups();
      if (entries.length === 0) {
        list.replaceChildren(h("li", { class: "muted" }, t("bk_empty")));
        return;
      }
      list.replaceChildren(
        ...entries.map((b) => {
          const restore = button(t("bk_restore"), { icon: "refresh" });
          restore.addEventListener("click", async () => {
            err.clear();
            try {
              if (await busy(restore, () => api.restoreBackup(b.id))) {
                toast(t("bk_restored"));
                onChanged();
                await load();
              }
            } catch (e) {
              err.show(errorMessage(e));
            }
          });
          return h("li", { class: "backup" }, h("span", null, icon("seal", 16), " ", t("bk_item", { date: formatDate(b.createdAt, getLang()), n: b.itemCount })), restore);
        }),
      );
    } catch (e) {
      err.show(errorMessage(e));
    }
  }
  void load();
  return h("section", { class: "stack" }, h("h3", { class: "subtitle" }, t("bk_title")), h("p", { class: "muted" }, t("bk_hint")), err.el, list);
}

// ── CSV (avançado) / CSV (advanced) ───────────────────────────────

function csvSection(): HTMLElement {
  const pw = passwordInput({ autocomplete: "current-password" });
  const err = errorBox();
  const go = button(t("csv_button"), { icon: "warning", variant: "danger" });
  go.addEventListener("click", async () => {
    err.clear();
    if (!pw.input.value) return err.show(t("required"));
    try {
      if (await busy(go, () => api.exportCsv(pw.input.value))) {
        pw.input.value = "";
        toast(t("csv_done"));
      }
    } catch (e) {
      err.show(errorMessage(e));
    }
  });
  return h(
    "details",
    { class: "advanced" },
    h("summary", null, t("csv_title")),
    h("div", { class: "stack" }, h("div", { class: "warn-box" }, h("p", null, icon("warning", 16), " ", t("csv_warn"))), field(t("master_password"), pw.wrap), err.el, h("div", { class: "actions" }, go)),
  );
}
