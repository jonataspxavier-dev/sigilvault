// Script que roda nas páginas (em todos os quadros): acha os campos de login, mostra o ícone do
// SigilVault e o balão (iframe da extensão), preenche quando o segundo plano manda e oferece
// salvar depois do envio. NUNCA preenche sozinho.
// Script that runs on pages (in all frames): finds login fields, shows the SigilVault icon and the
// balloon (extension iframe), fills when the background says so and offers to save after
// submitting. It NEVER fills by itself.

import { ext } from "../ext";
import { type FormContext, contextFor, fillLogin, fillNewPassword, isPasswordField, isUsernameLike, readCredentials } from "./forms";

const api = ext();
const ICON = 24;
const BALLOON_WIDTH = 330;
const PROMPT_WIDTH = 350;

type Mode = "icon" | "balloon" | "prompt";

interface Widget {
  token: string;
  host: HTMLElement;
  frame: HTMLIFrameElement;
  field: HTMLInputElement | null;
  ctx: FormContext | null;
  mode: Mode;
  height: number;
  observer: MutationObserver;
}

let widget: Widget | null = null;
let prompt: Widget | null = null;
/** Último formulário em que o usuário mexeu (para o aviso de salvar). / Last form the user touched (for the save prompt). */
let lastCtx: FormContext | null = null;
let lastSubmit = { at: 0, key: "" };
/** Nossa própria mudança no elemento (não é a página mexendo). / Our own change to the element (not the page tampering). */
let selfUpdate = false;

function token(): string {
  return [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function send(msg: Record<string, unknown>): Promise<Record<string, unknown>> {
  return (api.runtime.sendMessage(msg) as Promise<Record<string, unknown>>).catch(() => ({}));
}

// ── O elemento do balão / The balloon element ─────────────────────

/** Cria o iframe dentro de uma shadow root FECHADA, na camada superior da página.
 *  Creates the iframe inside a CLOSED shadow root, in the page's top layer. */
function createWidget(mode: Mode): Widget {
  const id = token();
  const host = document.createElement("sigilvault-ui");
  const shadow = host.attachShadow({ mode: "closed" });
  const frame = document.createElement("iframe");
  frame.src = `${api.runtime.getURL("ui/frame.html")}#${id}`;
  frame.setAttribute("allowtransparency", "true");
  frame.style.cssText = "all:initial;display:block;border:0;width:100%;height:100%;color-scheme:normal;background:transparent";
  shadow.appendChild(frame);
  const observer = new MutationObserver((records) => {
    if (selfUpdate) return;
    // A página mexeu no nosso elemento (moveu, escondeu…) ou o removeu: fecha por segurança.
    // Outras mudanças na página não importam.
    // The page touched our element (moved, hid…) or removed it: close for safety.
    // Other changes on the page do not matter.
    if (records.some((r) => r.type === "attributes") || !host.isConnected) closeWidget(w);
  });
  const w: Widget = { token: id, host, frame, field: null, ctx: null, mode, height: mode === "icon" ? ICON : 120, observer };
  return w;
}

function mountWidget(w: Widget): void {
  selfUpdate = true;
  document.documentElement.appendChild(w.host);
  try {
    w.host.setAttribute("popover", "manual");
    (w.host as HTMLElement & { showPopover?: () => void }).showPopover?.();
  } catch {
    /* sem camada superior: fica com z-index alto / no top layer: stays with a high z-index */
  }
  position(w);
  selfUpdate = false;
  w.observer.observe(w.host, { attributes: true });
  w.observer.observe(document.documentElement, { childList: true });
}

function closeWidget(w: Widget | null): void {
  if (!w) return;
  w.observer.disconnect();
  selfUpdate = true;
  w.host.remove();
  selfUpdate = false;
  void send({ cmd: "cs_unregister", token: w.token });
  if (widget === w) widget = null;
  if (prompt === w) prompt = null;
}

/** Posiciona: ícone dentro do campo, à direita; balão abaixo (ou acima); aviso no canto.
 *  Positions: icon inside the field, on the right; balloon below (or above); prompt in the corner. */
function position(w: Widget): void {
  if (!w.host.isConnected) return;
  let left = 0;
  let top = 0;
  let width = ICON;
  const height = w.mode === "icon" ? ICON : w.height;
  if (w.mode === "prompt") {
    width = PROMPT_WIDTH;
    left = window.innerWidth - width - 16;
    top = 16;
  } else if (w.field) {
    const r = w.field.getBoundingClientRect();
    if (w.mode === "icon") {
      left = r.right - ICON - 6;
      top = r.top + (r.height - ICON) / 2;
    } else {
      width = Math.min(BALLOON_WIDTH, window.innerWidth - 16);
      left = Math.max(8, Math.min(r.left, window.innerWidth - width - 8));
      top = r.bottom + height + 6 <= window.innerHeight ? r.bottom + 4 : Math.max(8, r.top - height - 4);
    }
  }
  const prev = selfUpdate;
  selfUpdate = true;
  w.host.style.cssText =
    `all:initial;position:fixed;left:${Math.round(left)}px;top:${Math.round(top)}px;width:${width}px;height:${height}px;` +
    "margin:0;padding:0;border:0;background:transparent;overflow:visible;z-index:2147483647;inset:auto;display:block";
  w.host.style.setProperty("left", `${Math.round(left)}px`);
  w.host.style.setProperty("top", `${Math.round(top)}px`);
  selfUpdate = prev;
}

let raf = 0;
function reposition(): void {
  cancelAnimationFrame(raf);
  raf = requestAnimationFrame(() => {
    if (widget) {
      // Campo sumiu da tela: fecha. / Field left the screen: close.
      if (widget.field && !widget.field.isConnected) return closeWidget(widget);
      position(widget);
    }
    if (prompt) position(prompt);
  });
}
window.addEventListener("scroll", reposition, { capture: true, passive: true });
window.addEventListener("resize", reposition, { passive: true });

// ── Ícone no campo / Icon in the field ────────────────────────────

async function showIcon(field: HTMLInputElement, ctx: FormContext): Promise<void> {
  if (widget?.field === field) return;
  closeWidget(widget);
  const w = createWidget("icon");
  w.field = field;
  w.ctx = ctx;
  widget = w;
  const r = await send({ cmd: "cs_register", token: w.token, kind: ctx.kind });
  if (!r.ok || widget !== w) return;
  mountWidget(w);
}

function targetInput(e: Event): HTMLInputElement | null {
  const el = e.composedPath()[0];
  return el instanceof HTMLInputElement ? el : null;
}

document.addEventListener(
  "focusin",
  (e) => {
    if (!e.isTrusted) return;
    const field = targetInput(e);
    if (!field || (!isPasswordField(field) && !isUsernameLike(field))) return;
    const ctx = contextFor(field);
    if (!ctx) return;
    lastCtx = ctx;
    void showIcon(field, ctx);
  },
  true,
);

document.addEventListener(
  "focusout",
  () => {
    // Fecha o ícone quando o foco vai para outro lugar (não para o próprio balão).
    // Closes the icon when focus goes elsewhere (not to the balloon itself).
    setTimeout(() => {
      if (!widget || widget.mode !== "icon") return;
      const active = document.activeElement;
      if (active === widget.field || active === widget.host) return;
      closeWidget(widget);
    }, 250);
  },
  true,
);

document.addEventListener(
  "keydown",
  (e) => {
    if (!e.isTrusted) return;
    if (e.key === "Escape" && widget?.mode === "balloon") closeWidget(widget);
    if (e.key === "Enter") {
      const field = targetInput(e);
      if (field && (isPasswordField(field) || isUsernameLike(field))) submitted(contextFor(field));
    }
  },
  true,
);

// ── Aviso de salvar / Save prompt ─────────────────────────────────

/** O formulário foi enviado: manda o que foi digitado para o app decidir se vale perguntar.
 *  The form was submitted: sends what was typed for the app to decide whether to ask. */
function submitted(ctx: FormContext | null): void {
  if (!ctx) return;
  const creds = readCredentials(ctx);
  if (!creds.password) return;
  const key = `${creds.username}\u0000${creds.password}`;
  const now = Date.now();
  if (key === lastSubmit.key && now - lastSubmit.at < 5000) return;
  lastSubmit = { at: now, key };
  void send({ cmd: "cs_submitted", username: creds.username, password: creds.password, kind: ctx.kind });
}

document.addEventListener(
  "submit",
  (e) => {
    if (!e.isTrusted || !(e.target instanceof HTMLFormElement)) return;
    const pw = [...e.target.querySelectorAll("input")].find(isPasswordField);
    submitted(pw ? contextFor(pw) : null);
  },
  true,
);

document.addEventListener(
  "click",
  (e) => {
    if (!e.isTrusted || !lastCtx) return;
    const el = e.composedPath()[0];
    if (!(el instanceof Element)) return;
    const button = el.closest('button, input[type="submit"], input[type="button"], [role="button"]');
    if (button && lastCtx.container.contains(button)) submitted(contextFor(lastCtx.passwords[0] ?? lastCtx.username!) ?? lastCtx);
  },
  true,
);

async function showSavePrompt(): Promise<void> {
  if (window !== window.top || prompt) return;
  const w = createWidget("prompt");
  prompt = w;
  const r = await send({ cmd: "cs_register", token: w.token, kind: "save" });
  if (!r.ok || prompt !== w) {
    prompt = null;
    return;
  }
  mountWidget(w);
}

// ── Ordens do segundo plano / Orders from the background ──────────

function widgetFor(t: unknown): Widget | null {
  if (widget && widget.token === t) return widget;
  if (prompt && prompt.token === t) return prompt;
  return null;
}

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  // Só o nosso segundo plano (nunca uma página). / Only our background (never a page).
  if (sender.id !== api.runtime.id || typeof msg !== "object" || msg === null) return;
  const m = msg as Record<string, unknown>;
  if (m.cmd === "bg_show_save") {
    void showSavePrompt();
    return;
  }
  const w = widgetFor(m.token);
  if (!w) return;
  switch (m.cmd) {
    case "bg_resize":
      w.mode = m.mode as Mode;
      w.height = typeof m.height === "number" ? m.height : w.height;
      position(w);
      sendResponse({ ok: true });
      return;
    case "bg_close":
      closeWidget(w);
      sendResponse({ ok: true });
      return;
    case "bg_fill": {
      // Recalcula o formulário agora: só campos ainda visíveis. / Recomputes the form now: only fields still visible.
      const ctx = w.field ? contextFor(w.field) : null;
      const filled = ctx ? fillLogin(ctx, String(m.username ?? ""), String(m.password ?? "")) : 0;
      closeWidget(w);
      sendResponse({ filled });
      return;
    }
    case "bg_fill_new": {
      const ctx = w.field ? contextFor(w.field) : null;
      sendResponse({ filled: ctx ? fillNewPassword(ctx, String(m.password ?? "")) : 0 });
      return;
    }
    case "bg_read": {
      const ctx = w.field ? contextFor(w.field) : null;
      sendResponse(ctx ? readCredentials(ctx) : {});
      return;
    }
  }
});

// Página nova: há um "Salvar?" pendente desta aba? / New page: is there a pending "Save?" for this tab?
if (window === window.top) {
  void send({ cmd: "cs_ready" }).then((r) => {
    if (r.pendingSave) void showSavePrompt();
  });
}
