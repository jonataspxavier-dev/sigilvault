// Roteador do segundo plano para a Fase 5: liga o script das páginas, o balão (iframe da
// extensão) e o app. Decide QUEM pode pedir O QUÊ.
// Background router for Phase 5: links the page script, the balloon (extension iframe) and the
// app. Decides WHO may ask for WHAT.
//
// Regras / Rules:
// - Só o script da página registra formulários; a origem vem do navegador (`sender`).
//   Only the page script registers forms; the origin comes from the browser (`sender`).
// - Só o balão aberto para aquele registro (mesma aba, primeiro quadro que se apresentou) pode
//   pedir ações. / Only the balloon opened for that registration (same tab, first frame that
//   introduced itself) may ask for actions.
// - A senha vai do app direto para o quadro da página; nunca passa pelo balão.
//   The password goes from the app straight to the page frame; it never passes through the balloon.

import type { MessageSender } from "./ext";

type Msg = Record<string, unknown>;

export type FormKind = "login" | "signup" | "save";

interface Registration {
  tabId: number;
  frameId: number;
  origin: string;
  top: string;
  kind: FormKind;
  uiFrameId?: number;
  created: number;
}

interface PendingSave {
  origin: string;
  top: string;
  username: string;
  password: string;
  status: "new" | "changed";
  item?: { id: string; title: string; username: string };
  site: string;
  expires: number;
}

export interface RouterDeps {
  /** Operação no app. / Operation in the app. */
  request(op: string, extra?: Msg, timeoutMs?: number): Promise<Msg>;
  /** Mensagem para um quadro de uma aba. / Message to a frame of a tab. */
  sendToFrame(tabId: number, frameId: number, msg: Msg): Promise<unknown>;
  openApp(): Promise<unknown>;
  /** URL base da página do balão (`chrome-extension://…/ui/frame.html`). / Balloon page base URL. */
  uiUrl: string;
  now?: () => number;
}

const SAVE_TTL_MS = 3 * 60_000;
const MAX_REGISTRATIONS = 300;
const FILL_TIMEOUT_MS = 120_000;

/** Origem http(s) de quem mandou, ou null. / Sender's http(s) origin, or null. */
export function senderOrigin(sender: MessageSender): string | null {
  let origin = sender.origin;
  if (!origin || origin === "null") {
    try {
      origin = sender.url ? new URL(sender.url).origin : undefined;
    } catch {
      origin = undefined;
    }
  }
  return origin && /^https?:\/\//.test(origin) ? origin : null;
}

function topOrigin(sender: MessageSender): string | null {
  try {
    const o = sender.tab?.url ? new URL(sender.tab.url).origin : null;
    return o && /^https?:\/\//.test(o) ? o : null;
  } catch {
    return null;
  }
}

const str = (v: unknown, max = 1024): string | null => (typeof v === "string" && v.length <= max ? v : null);

export class Router {
  private regs = new Map<string, Registration>();
  private pending = new Map<number, PendingSave>();

  constructor(private readonly deps: RouterDeps) {}

  private now(): number {
    return (this.deps.now ?? Date.now)();
  }

  /** Aba fechada: esquece tudo dela (inclusive senha pendente). / Tab closed: forgets everything (including a pending password). */
  tabClosed(tabId: number): void {
    for (const [token, r] of this.regs) if (r.tabId === tabId) this.regs.delete(token);
    this.pending.delete(tabId);
  }

  private pendingFor(tabId: number): PendingSave | null {
    const p = this.pending.get(tabId);
    if (p && p.expires < this.now()) {
      this.pending.delete(tabId);
      return null;
    }
    return p ?? null;
  }

  // ── Script da página / Page script ──────────────────────────────

  async fromContent(msg: Msg, sender: MessageSender): Promise<Msg> {
    const tabId = sender.tab?.id;
    const frameId = sender.frameId ?? 0;
    const origin = senderOrigin(sender);
    const top = topOrigin(sender);
    if (tabId === undefined || !origin || !top) return { error: "bad_sender" };

    switch (msg.cmd) {
      case "cs_register": {
        const token = str(msg.token, 64);
        const kind = msg.kind;
        if (!token || !/^[0-9a-f]{32}$/.test(token) || (kind !== "login" && kind !== "signup" && kind !== "save")) return { error: "bad_message" };
        if (kind === "save" && (frameId !== 0 || !this.pendingFor(tabId))) return { error: "no_pending" };
        if (this.regs.size >= MAX_REGISTRATIONS) this.regs.delete(this.regs.keys().next().value!);
        this.regs.set(token, { tabId, frameId, origin, top, kind, created: this.now() });
        return { ok: true };
      }
      case "cs_unregister": {
        const token = str(msg.token, 64);
        const r = token ? this.regs.get(token) : undefined;
        if (token && r && r.tabId === tabId && r.frameId === frameId) this.regs.delete(token);
        return { ok: true };
      }
      case "cs_ready":
        return { pendingSave: frameId === 0 && this.pendingFor(tabId) !== null };
      case "cs_submitted": {
        const username = str(msg.username, 512) ?? "";
        const password = str(msg.password, 1024);
        if (!password) return { ok: false };
        const r = await this.deps.request("check_save", { origin, top, username, password });
        if (r.status !== "new" && r.status !== "changed") return { ok: true, prompt: false };
        this.pending.set(tabId, {
          origin,
          top,
          username,
          password,
          status: r.status,
          item: r.item as PendingSave["item"],
          site: typeof r.site === "string" ? r.site : new URL(origin).host,
          expires: this.now() + SAVE_TTL_MS,
        });
        // Mostra no quadro principal da aba (a página pode estar trocando). / Shows in the tab's main frame (the page may be changing).
        await this.deps.sendToFrame(tabId, 0, { cmd: "bg_show_save" }).catch(() => undefined);
        return { ok: true, prompt: true };
      }
      default:
        return { error: "unknown" };
    }
  }

  // ── Balão (iframe da extensão) / Balloon (extension iframe) ─────

  /** O balão é legítimo para este registro? / Is the balloon legitimate for this registration? */
  private ownerOf(msg: Msg, sender: MessageSender, claim: boolean): Registration | null {
    if (!(sender.url ?? "").startsWith(this.deps.uiUrl)) return null;
    const token = str(msg.token, 64);
    const r = token ? this.regs.get(token) : undefined;
    if (!r || sender.tab?.id !== r.tabId || sender.frameId === undefined) return null;
    if (r.uiFrameId === undefined) {
      if (!claim) return null;
      r.uiFrameId = sender.frameId;
    }
    return r.uiFrameId === sender.frameId ? r : null;
  }

  async fromUi(msg: Msg, sender: MessageSender): Promise<Msg> {
    const r = this.ownerOf(msg, sender, msg.cmd === "ui_hello");
    if (!r) return { error: "forbidden" };
    const token = msg.token as string;
    const toPage = (m: Msg) => this.deps.sendToFrame(r.tabId, r.frameId, { ...m, token });

    switch (msg.cmd) {
      case "ui_hello": {
        const site = new URL(r.origin).host;
        if (r.kind !== "save") return { kind: r.kind, site, secure: r.origin.startsWith("https:"), crossOrigin: r.origin !== r.top };
        const p = this.pendingFor(r.tabId);
        if (!p) return { error: "no_pending" };
        // Nunca a senha: só o que o aviso precisa mostrar. / Never the password: only what the prompt needs to show.
        return { kind: "save", site: p.site, save: { username: p.username, status: p.status, title: p.item?.title ?? null } };
      }
      case "ui_size": {
        const mode = msg.mode;
        const height = typeof msg.height === "number" ? Math.max(24, Math.min(600, Math.round(msg.height))) : 0;
        if (mode !== "icon" && mode !== "balloon" && mode !== "prompt") return { error: "bad_message" };
        await toPage({ cmd: "bg_resize", mode, height }).catch(() => undefined);
        return { ok: true };
      }
      case "ui_close":
        await toPage({ cmd: "bg_close" }).catch(() => undefined);
        if (r.kind === "save") this.pending.delete(r.tabId);
        return { ok: true };
      case "ui_logins":
        return this.deps.request("logins", { origin: r.origin, top: r.top });
      case "ui_fill": {
        const id = str(msg.id, 64);
        if (!id || r.kind === "save") return { error: "bad_message" };
        const res = await this.deps.request("fill", { id, origin: r.origin, top: r.top }, FILL_TIMEOUT_MS);
        if (typeof res.password !== "string") return res;
        const filled = (await toPage({ cmd: "bg_fill", username: res.username ?? "", password: res.password }).catch(() => ({ filled: 0 }))) as Msg;
        return { ok: true, filled: filled?.filled ?? 0 };
      }
      case "ui_get_generator":
        return this.deps.request("get_generator");
      case "ui_generate":
        return this.deps.request("generate", typeof msg.options === "object" && msg.options ? { options: msg.options } : {});
      case "ui_use_password": {
        const password = str(msg.password, 256);
        if (!password || r.kind !== "signup") return { error: "bad_message" };
        const filled = ((await toPage({ cmd: "bg_fill_new", password }).catch(() => null)) as Msg | null)?.filled ?? 0;
        if (!filled) return { error: "no_fields" };
        const fields = ((await toPage({ cmd: "bg_read" }).catch(() => null)) as Msg | null) ?? {};
        if (typeof msg.options === "object" && msg.options) await this.deps.request("save_generator", { options: msg.options });
        return this.deps.request("save", { origin: r.origin, top: r.top, username: str(fields.username, 512) ?? "", password });
      }
      case "ui_just_save": {
        if (r.kind !== "signup") return { error: "bad_message" };
        const fields = ((await toPage({ cmd: "bg_read" }).catch(() => null)) as Msg | null) ?? {};
        const username = str(fields.username, 512) ?? "";
        const password = str(fields.password, 1024);
        if (!password) return { error: "empty_password" };
        const check = await this.deps.request("check_save", { origin: r.origin, top: r.top, username, password });
        if (check.status === "same") return { saved: true, outcome: "same" };
        if (check.locked || check.error) return check;
        const replace = check.status === "changed" ? (check.item as { id?: string } | undefined)?.id : undefined;
        return this.deps.request("save", { origin: r.origin, top: r.top, username, password, ...(replace ? { replace } : {}) });
      }
      case "ui_save_decision": {
        if (r.kind !== "save") return { error: "bad_message" };
        const p = this.pendingFor(r.tabId);
        if (!p || msg.save !== true) {
          this.pending.delete(r.tabId);
          return { ok: true };
        }
        const res = await this.deps.request("save", {
          origin: p.origin,
          top: p.top,
          username: p.username,
          password: p.password,
          ...(p.status === "changed" && p.item ? { replace: p.item.id } : {}),
        });
        // Cofre trancado ou falha: o pedido continua, para tentar de novo após desbloquear.
        // Locked vault or failure: the request stays, to try again after unlocking.
        if (res.saved) this.pending.delete(r.tabId);
        return res;
      }
      case "ui_open_app":
        await this.deps.openApp();
        return { ok: true };
      default:
        return { error: "unknown" };
    }
  }
}
