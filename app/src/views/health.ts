// Relatório de saúde: senhas fracas, repetidas ou antigas. Nenhuma senha chega a esta tela.
// Health report: weak, reused or old passwords. No password ever reaches this screen.

import { api, type HealthEntry, type HealthReport } from "../api";
import { h, mount } from "../dom";
import { t } from "../i18n";
import { button, errorMessage } from "../ui";

/** Letra do grupo de senha repetida (A, B, …, Z, AA…). / Letter of the reused-password group. */
function groupLetter(n: number): string {
  let s = "";
  n += 1;
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

function badges(e: HealthEntry): HTMLElement[] {
  const out: HTMLElement[] = [];
  if (e.weak) out.push(h("span", { class: "tag health-bad" }, t("health_weak")));
  if (e.reusedWith > 0 && e.reuseGroup !== null) out.push(h("span", { class: "tag health-bad" }, t("health_reused", { group: groupLetter(e.reuseGroup), n: e.reusedWith })));
  if (e.old) out.push(h("span", { class: "tag health-warn" }, t("health_old", { days: e.ageDays })));
  return out;
}

function stat(label: string, value: number, bad: boolean): HTMLElement {
  return h("div", { class: bad && value > 0 ? "health-stat bad" : "health-stat" }, h("span", { class: "health-num" }, String(value)), h("span", { class: "muted small" }, label));
}

function render(body: HTMLElement, r: HealthReport, onOpen: (id: string) => void): void {
  const weak = r.entries.filter((e) => e.weak).length;
  const reused = r.entries.filter((e) => e.reusedWith > 0).length;
  const old = r.entries.filter((e) => e.old).length;
  const summary = h("div", { class: "health-stats" }, stat(t("health_stat_weak"), weak, true), stat(t("health_stat_reused"), reused, true), stat(t("health_stat_old"), old, false));
  if (r.checked === 0) return mount(body, h("p", { class: "muted" }, t("health_none")));
  if (r.entries.length === 0) return mount(body, summary, h("p", { class: "health-ok" }, t("health_all_good", { n: r.checked })));
  const list = h(
    "ul",
    { class: "item-list health-list" },
    ...r.entries.map((e) =>
      h(
        "li",
        { class: "item", tabIndex: 0, onClick: () => onOpen(e.id), onKeydown: (ev: KeyboardEvent) => ev.key === "Enter" && onOpen(e.id) },
        h("span", { class: "item-text" }, h("span", { class: "item-title" }, e.title), h("span", { class: "item-sub" }, e.username || " ")),
        h("span", { class: "tags" }, ...badges(e)),
      ),
    ),
  );
  mount(body, summary, h("p", { class: "muted small" }, t("health_checked", { n: r.checked, bad: r.entries.length })), list);
}

/** Painel do relatório. `onOpen` abre o item para trocar a senha. / Report panel. `onOpen` opens the item to change its password. */
export function healthPanel(onOpen: (id: string) => void): HTMLElement {
  const body = h("div", { class: "stack" }, h("p", { class: "muted" }, t("health_loading")));
  const load = async () => {
    try {
      render(body, await api.healthReport(), onOpen);
    } catch (e) {
      mount(body, h("p", { class: "error" }, errorMessage(e)));
    }
  };
  void load();
  return h(
    "div",
    { class: "stack" },
    h("h2", { class: "title" }, t("health_title")),
    h("p", { class: "muted" }, t("health_intro")),
    body,
    h("div", { class: "actions" }, button(t("health_refresh"), { icon: "refresh", onClick: () => void load() })),
  );
}
