// O balão do SigilVault: página da extensão dentro de um iframe na página do site. A página do
// site não consegue ler nem clicar aqui dentro. A senha salva NUNCA chega a este balão: o
// segundo plano a entrega direto ao campo.
// The SigilVault balloon: an extension page inside an iframe on the site's page. The site cannot
// read or click in here. The saved password NEVER reaches this balloon: the background hands it
// straight to the field.

import "./frame.css";
import "../medieval.css";

import { type Child, h, mount } from "../dom";
import { ext } from "../ext";
import { type Key, t } from "../i18n";

type Msg = Record<string, unknown>;
type Mode = "icon" | "balloon" | "prompt";

interface Options {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

const api = ext();
const token = location.hash.slice(1);
const root = document.getElementById("ui")!;

let mode: Mode = "icon";
let kind: "login" | "signup" | "save" = "login";
let site = "";
let secure = true;
let crossOrigin = false;

// ── Proteção contra clickjacking / Clickjacking protection ───────

/** Momento da última troca de conteúdo do balão. Cliques só valem 500 ms depois: um botão que
 *  acabou de surgir embaixo do cursor não pode ser clicado "sem querer".
 *  Moment of the balloon's last content change. Clicks only count 500 ms later: a button that just
 *  appeared under the cursor cannot be clicked "by accident". */
let shownAt = Date.now();
/** Chrome/Brave/Opera: o navegador diz se o iframe está de fato visível (sem cobertura, sem
 *  transparência, sem distorção). No Firefox não existe; fica `true`.
 *  Chrome/Brave/Opera: the browser says whether the iframe is truly visible (not covered, not
 *  transparent, not distorted). Firefox lacks it; stays `true`. */
let reallyVisible = true;
try {
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const v = (e as IntersectionObserverEntry & { isVisible?: boolean }).isVisible;
        reallyVisible = v === undefined ? true : v;
      }
    },
    { threshold: [1], trackVisibility: true, delay: 100 } as IntersectionObserverInit,
  );
  io.observe(document.documentElement);
} catch {
  /* navegador sem IntersectionObserver v2 / browser without IntersectionObserver v2 */
}

/** Só aceita clique real, depois de 500 ms visível, com o balão de fato visível.
 *  Only accepts a real click, after 500 ms on screen, with the balloon truly visible. */
function guarded(action: () => void): (e: Event) => void {
  return (e) => {
    if (!e.isTrusted || Date.now() - shownAt < 500 || !reallyVisible) return;
    action();
  };
}

function call(cmd: string, extra: Msg = {}): Promise<Msg> {
  return (api.runtime.sendMessage({ cmd, token, ...extra }) as Promise<Msg>).then((r) => r ?? {}, () => ({ error: "not_connected" }));
}

/** Avisa o tamanho para a página ajustar o iframe. / Reports the size so the page adjusts the iframe. */
function resize(next: Mode): void {
  mode = next;
  document.body.dataset.mode = mode;
  requestAnimationFrame(() => {
    const height = mode === "icon" ? 24 : Math.ceil(document.getElementById("ui")!.getBoundingClientRect().height) + 2;
    void call("ui_size", { mode, height });
  });
}

function close(): void {
  void call("ui_close");
}

// ── Peças / Pieces ────────────────────────────────────────────────

/** Selo de cera com "S" (desenho original). / Wax seal with an "S" (original drawing). */
function seal(size = 20): SVGSVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("aria-hidden", "true");
  const parts: [string, Record<string, string>][] = [
    ["path", { d: "M12 2.5c1.3 0 1.9 1.1 3 1.4s2.3-.2 3.2.7.4 2 .7 3.2 1.4 1.7 1.4 3-1.1 1.9-1.4 3 .2 2.3-.7 3.2-2 .4-3.2.7-1.7 1.4-3 1.4-1.9-1.1-3-1.4-2.3.2-3.2-.7-.4-2-.7-3.2S2.5 13.3 2.5 12s1.1-1.9 1.4-3-.2-2.3.7-3.2 2-.4 3.2-.7S10.7 2.5 12 2.5z", fill: "#9b2226", stroke: "#4a0e10", "stroke-width": "1" }],
    ["circle", { cx: "12", cy: "12", r: "6.2", fill: "none", stroke: "#c0393e", "stroke-width": "1" }],
    ["path", { d: "M14.6 9.2c-.5-.9-1.5-1.3-2.6-1.3-1.4 0-2.5.8-2.5 2 0 2.6 5.2 1.5 5.2 4.2 0 1.3-1.2 2.1-2.7 2.1-1.2 0-2.2-.5-2.7-1.4", fill: "none", stroke: "#e6c784", "stroke-width": "1.6", "stroke-linecap": "round" }],
  ];
  for (const [tag, attrs] of parts) {
    const el = document.createElementNS(ns, tag);
    for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
    svg.appendChild(el);
  }
  return svg;
}

function btn(label: string, variant: "primary" | "secondary" | "ghost", action: () => void): HTMLButtonElement {
  return h("button", { type: "button", class: `btn btn-${variant}`, onClick: guarded(action) }, label);
}

function header(): HTMLElement {
  return h(
    "header",
    { class: "head" },
    seal(18),
    h("span", { class: "site", title: site }, site),
    h("button", { type: "button", class: "x", "aria-label": t("ui_close"), onClick: guarded(close) }, "×"),
  );
}

function warnings(): Child[] {
  return [!secure ? h("p", { class: "warn" }, t("ui_insecure")) : null, crossOrigin ? h("p", { class: "warn" }, t("ui_cross_origin")) : null];
}

function note(key: Key, cls = "muted", vars: Record<string, string> = {}): HTMLElement {
  return h("p", { class: cls }, t(key, vars));
}

/** Mensagem de erro do app traduzida. / Translated app error message. */
function problem(r: Msg): HTMLElement[] {
  const code = typeof r.error === "string" ? r.error : r.locked ? "locked" : "generic";
  if (code === "locked") return [note("ui_locked", "warn"), h("div", { class: "actions" }, btn(t("open_app"), "primary", () => void call("ui_open_app")))];
  const map: Record<string, Key> = {
    not_paired: "ui_not_paired",
    not_connected: "ui_no_app",
    no_host: "ui_no_app",
    app_unavailable: "ui_no_app",
    denied: "ui_denied",
    not_allowed: "ui_not_allowed",
    rate_limited: "ui_rate_limited",
    no_fields: "ui_no_fields",
    empty_password: "ui_empty_password",
  };
  return [note(map[code] ?? "ui_generic", "warn")];
}

function show(...content: Child[]): void {
  shownAt = Date.now();
  mount(root, h("section", { class: "card" }, header(), ...warnings(), ...content));
  resize(mode === "prompt" ? "prompt" : "balloon");
}

// ── Ícone / Icon ──────────────────────────────────────────────────

function renderIcon(): void {
  shownAt = Date.now();
  mount(root, h("button", { type: "button", class: "icon", title: t("ui_icon"), "aria-label": t("ui_icon"), onClick: guarded(open) }, seal(20)));
  resize("icon");
}

function open(): void {
  if (kind === "signup") void renderSignup("generate");
  else void renderLogin();
}

// ── Login / Login ─────────────────────────────────────────────────

async function renderLogin(): Promise<void> {
  show(note("ui_loading"));
  const r = await call("ui_logins");
  const logins = Array.isArray(r.logins) ? (r.logins as { id: string; title: string; username: string }[]) : null;
  if (!logins) return show(...problem(r));
  if (logins.length === 0) return show(note("ui_none", "muted", { site }));
  const status = h("div", { class: "status" });
  show(
    note("ui_pick"),
    h(
      "ul",
      { class: "accounts" },
      ...logins.map((l) =>
        h(
          "li",
          null,
          h(
            "button",
            {
              type: "button",
              class: "account",
              onClick: guarded(async () => {
                status.replaceChildren(note("ui_confirm_in_app"));
                resize("balloon");
                const res = await call("ui_fill", { id: l.id });
                // Deu certo: a página já fechou o balão. / It worked: the page already closed the balloon.
                if (res.ok && Number(res.filled) > 0) return;
                status.replaceChildren(...(res.ok ? problem({ error: "no_fields" }) : problem(res)));
                resize("balloon");
              }),
            },
            h("strong", null, l.title),
            l.username ? h("span", { class: "muted" }, l.username) : null,
          ),
        ),
      ),
    ),
    status,
  );
}

// ── Cadastro: gerar ou só salvar / Sign-up: generate or just save ─

let options: Options | null = null;
let current = "";

function strengthLabel(bits: number): string {
  if (bits >= 128) return t("ui_strength_max");
  if (bits >= 90) return t("ui_strength_great");
  if (bits >= 60) return t("ui_strength_good");
  return t("ui_strength_weak");
}

function tabs(active: "generate" | "save"): HTMLElement {
  const tab = (id: "generate" | "save", key: Key) =>
    h("button", { type: "button", class: `tab${active === id ? " active" : ""}`, onClick: guarded(() => void renderSignup(id)) }, t(key));
  return h("nav", { class: "tabs" }, tab("generate", "ui_tab_generate"), tab("save", "ui_tab_save"));
}

async function renderSignup(which: "generate" | "save"): Promise<void> {
  if (which === "save") {
    const status = h("div", { class: "status" });
    return show(
      tabs("save"),
      note("ui_just_save_intro"),
      status,
      h(
        "div",
        { class: "actions" },
        btn(t("ui_just_save_button"), "primary", async () => {
          const r = await call("ui_just_save");
          if (r.saved) {
            status.replaceChildren(note(r.outcome === "same" ? "ui_already" : r.outcome === "updated" ? "ui_updated" : "ui_saved", "ok"));
            resize("balloon");
            setTimeout(close, 1800);
          } else {
            status.replaceChildren(...problem(r));
            resize("balloon");
          }
        }),
      ),
    );
  }

  if (!options) {
    show(tabs("generate"), note("ui_loading"));
    const r = await call("ui_get_generator");
    if (!r.options) return show(tabs("generate"), ...problem(r));
    options = r.options as Options;
  }
  const opts = options;
  const preview = h("div", { class: "preview" });
  const meter = h("div", { class: "muted" });
  const status = h("div", { class: "status" });

  const regenerate = async () => {
    const r = await call("ui_generate", { options: opts });
    if (typeof r.password !== "string") {
      preview.replaceChildren();
      status.replaceChildren(...problem(r));
      return resize("balloon");
    }
    current = r.password;
    preview.textContent = current;
    const bits = Math.round(Number(r.entropyBits) || 0);
    meter.textContent = `${t("ui_bits", { bits: String(bits) })} · ${strengthLabel(bits)}`;
    status.replaceChildren();
    resize("balloon");
  };

  const toggle = (key: keyof Omit<Options, "length">, label: Key) => {
    const box = h("input", { type: "checkbox", checked: opts[key] });
    box.addEventListener("change", () => {
      opts[key] = box.checked;
      void regenerate();
    });
    return h("label", { class: "check" }, box, h("span", null, t(label)));
  };
  const length = h("input", { type: "range", min: 8, max: 64, value: String(Math.min(64, opts.length)) });
  const lengthValue = h("span", { class: "len" }, String(opts.length));
  length.addEventListener("input", () => {
    opts.length = Number(length.value);
    lengthValue.textContent = length.value;
    void regenerate();
  });

  show(
    tabs("generate"),
    preview,
    meter,
    h("label", { class: "range" }, h("span", null, t("ui_length")), length, lengthValue),
    h("div", { class: "checks" }, toggle("uppercase", "ui_upper"), toggle("lowercase", "ui_lower"), toggle("digits", "ui_digits"), toggle("symbols", "ui_symbols"), toggle("excludeAmbiguous", "ui_ambiguous")),
    status,
    h(
      "div",
      { class: "actions" },
      btn(t("ui_another"), "secondary", () => void regenerate()),
      btn(t("ui_use_save"), "primary", async () => {
        if (!current) return;
        const r = await call("ui_use_password", { password: current, options: opts });
        if (r.saved) {
          status.replaceChildren(note("ui_used_saved", "ok"));
          resize("balloon");
          setTimeout(close, 1800);
        } else {
          status.replaceChildren(...problem(r));
          resize("balloon");
        }
      }),
    ),
  );
  await regenerate();
}

// ── Aviso "Salvar?" / "Save?" prompt ─────────────────────────────

function renderPrompt(save: { username: string; status: string; title: string | null }): void {
  mode = "prompt";
  const changed = save.status === "changed";
  const question = changed
    ? t("ui_save_changed", { title: save.title ?? site, user: save.username })
    : save.username
      ? t("ui_save_new", { user: save.username, site })
      : t("ui_save_new_nouser", { site });
  const status = h("div", { class: "status" });
  show(
    h("p", { class: "question" }, question),
    changed ? note("ui_save_history") : null,
    status,
    h(
      "div",
      { class: "actions" },
      btn(t("ui_not_now"), "ghost", async () => {
        await call("ui_save_decision", { save: false });
        close();
      }),
      btn(changed ? t("ui_update_button") : t("ui_save_button"), "primary", async () => {
        const r = await call("ui_save_decision", { save: true });
        if (r.saved) {
          status.replaceChildren(note(changed ? "ui_updated" : "ui_saved", "ok"));
          resize("prompt");
          setTimeout(close, 1500);
        } else {
          status.replaceChildren(...problem(r));
          resize("prompt");
        }
      }),
    ),
  );
}

// ── Início / Start ────────────────────────────────────────────────

void call("ui_hello").then((r) => {
  if (r.error) return close();
  kind = r.kind as typeof kind;
  site = typeof r.site === "string" ? r.site : "";
  secure = r.secure !== false;
  crossOrigin = r.crossOrigin === true;
  if (kind === "save" && r.save) renderPrompt(r.save as { username: string; status: string; title: string | null });
  else renderIcon();
});
