// Popup da extensão: mostra a ligação com o app e conduz o pareamento. Não guarda nada: pede
// tudo ao segundo plano.
// Extension popup: shows the link with the app and guides pairing. It stores nothing: it asks
// the background for everything.

import "./popup.css";

import type { Health, View } from "../controller";
import { type Child, h, mount } from "../dom";
import { ext } from "../ext";
import { formatCode } from "../format";
import { type Key, errorText, t } from "../i18n";

const api = ext();
const root = document.getElementById("app")!;

let view: View | null = null;
let busy = false;
let confirmUnpair = false;
let justPaired = false;

async function send(cmd: string, extra: Record<string, unknown> = {}): Promise<void> {
  busy = true;
  render();
  try {
    const wasPaired = view?.paired ?? false;
    view = (await api.runtime.sendMessage({ cmd, ...extra })) as View;
    justPaired = !wasPaired && view.paired;
  } finally {
    busy = false;
    render();
  }
}

/** Texto de cada problema de ligação. / Text for each link problem. */
const HEALTH_TEXT: Record<Exclude<Health, "ok" | "unknown">, Key> = {
  no_host: "health_no_host",
  app_unavailable: "health_app_unavailable",
  untrusted_app: "health_untrusted_app",
  unknown_extension: "health_unknown_extension",
};

function btn(label: string, variant: "primary" | "secondary" | "ghost" | "danger", onClick: () => void, type: "button" | "submit" = "button"): HTMLButtonElement {
  return h("button", { type, class: `btn btn-${variant}`, disabled: busy, onClick }, label);
}

function errorBox(code: string | null | undefined): Child {
  return code ? h("p", { class: "error", role: "alert" }, errorText(code)) : null;
}

function statusPill(v: View | null): Child {
  if (!v?.paired || !v.connected || v.locked === null) return h("span", { class: "pill pill-off" }, "●");
  return h("span", { class: `pill ${v.locked ? "pill-locked" : "pill-unlocked"}` }, v.locked ? t("status_locked") : t("status_unlocked"));
}

function body(v: View | null): Child[] {
  if (!v || (busy && v.health === "unknown")) return [h("p", { class: "muted" }, t("connecting"))];

  if (v.health !== "ok" && v.health !== "unknown") {
    return [h("p", { class: "error", role: "alert" }, t(HEALTH_TEXT[v.health])), h("div", { class: "actions" }, btn(t("retry"), "secondary", () => void send("view")))];
  }

  if (!v.paired && v.pairing === "waiting_code") {
    const input = h("input", { class: "code-input", maxLength: 9, autocomplete: "off", spellcheck: false, "aria-label": t("code_label"), placeholder: "XXXX-XXXX" });
    input.addEventListener("input", () => (input.value = formatCode(input.value)));
    const form = h(
      "form",
      { class: "stack" },
      h("p", null, t("code_intro")),
      h("label", { class: "field-label" }, t("code_label")),
      input,
      errorBox(v.error),
      h("div", { class: "actions" }, btn(t("cancel"), "ghost", () => void send("cancel")), btn(t("confirm"), "primary", () => undefined, "submit")),
    );
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      const code = input.value;
      input.value = "";
      void send("code", { code });
    });
    queueMicrotask(() => input.focus());
    return [form];
  }

  if (!v.paired) {
    return [h("h1", { class: "title" }, t("pair_title")), h("p", { class: "muted" }, t("pair_intro")), errorBox(v.error), h("div", { class: "actions" }, btn(t("pair_button"), "primary", () => void send("pair")))];
  }

  if (!v.connected || v.locked === null) {
    return [errorBox(v.error ?? "generic"), h("div", { class: "actions" }, btn(t("retry"), "secondary", () => void send("view")))];
  }

  const status = v.locked
    ? [h("h1", { class: "title" }, t("status_locked")), h("p", { class: "muted" }, t("status_locked_hint"))]
    : [h("h1", { class: "title" }, v.username ? t("status_unlocked_user", { name: v.username }) : t("status_unlocked"))];
  return [
    ...status,
    justPaired ? h("p", { class: "ok" }, t("paired_ok")) : null,
    h("div", { class: "actions" }, btn(t("open_app"), v.locked ? "primary" : "secondary", () => void send("open"))),
    h("hr", { class: "divider" }),
    confirmUnpair
      ? h("div", { class: "actions" }, btn(t("cancel"), "ghost", () => ((confirmUnpair = false), render())), btn(t("unpair_confirm"), "danger", () => ((confirmUnpair = false), void send("unpair"))))
      : h("button", { type: "button", class: "link", disabled: busy, onClick: () => ((confirmUnpair = true), render()) }, t("unpair")),
  ];
}

function render(): void {
  mount(
    root,
    h(
      "section",
      { class: "card" },
      h("header", { class: "brand" }, h("img", { src: "../icons/logo/48x48.png", alt: "", class: "logo" }), h("span", { class: "brand-name" }, "SigilVault"), statusPill(view)),
      ...body(view),
      h("p", { class: "footer" }, t("footer")),
    ),
  );
}

render();
void send("view");
