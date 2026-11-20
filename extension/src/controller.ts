// Máquina de estados da conexão com o app: conectar, parear, autenticar, consultar o estado.
// Não usa APIs do navegador diretamente (elas chegam por `Deps`), para poder ser testada no Node
// e contra o app de verdade.
// Connection state machine with the app: connect, pair, authenticate, query status.
// It does not use browser APIs directly (they come through `Deps`), so it can be tested in Node
// and against the real app.

import { Channel, importPairingKey } from "./channel";
import type { PortLike } from "./ext";
import { fromHex, randomBytes, toHex } from "./hex";
import type { KeyStore, StoredPairing } from "./keystore";
import { MalformedCodeError, type Pake } from "./pake";

/** Situação da ligação com o app. / State of the link with the app. */
export type Health = "unknown" | "no_host" | "app_unavailable" | "untrusted_app" | "unknown_extension" | "ok";

/** O que o popup mostra. / What the popup shows. */
export interface View {
  health: Health;
  paired: boolean;
  /** Canal criptografado aberto com o app. / Encrypted channel open with the app. */
  connected: boolean;
  locked: boolean | null;
  username: string | null;
  pairing: "idle" | "waiting_code";
  /** Código do último erro (o popup traduz). / Last error code (the popup translates). */
  error: string | null;
}

export type IconState = "off" | "locked" | "unlocked";

export function iconState(v: View): IconState {
  if (!v.paired || !v.connected || v.locked === null) return "off";
  return v.locked ? "locked" : "unlocked";
}

export interface Deps {
  /** Abre a porta do Native Messaging. / Opens the Native Messaging port. */
  connect(): PortLike;
  /** Motivo da desconexão informado pelo navegador. / Disconnect reason given by the browser. */
  disconnectReason(port: PortLike): string | undefined;
  pake(): Promise<Pake>;
  store: KeyStore;
  browserName: string;
  onChange(view: View): void;
}

type Msg = Record<string, unknown>;

const REPLY_TIMEOUT_MS = 20_000;

/** A ligação caiu ou o app recusou. / The link dropped or the app refused. */
export class LinkError extends Error {}

interface Waiter {
  resolve(m: Msg): void;
  reject(e: Error): void;
  timer: ReturnType<typeof setTimeout>;
}

/** Uma conexão com o app (uma porta do Native Messaging). / One connection with the app (one Native Messaging port). */
class Link {
  channel: Promise<Channel> | null = null;
  closed = false;
  received = false;
  hostError: string | null = null;
  private plainWaiters: Waiter[] = [];
  private boxWaiters = new Map<number, Waiter>();
  private nextRid = 1;
  private inbox: Promise<void> = Promise.resolve();
  private outbox: Promise<void> = Promise.resolve();

  constructor(
    private readonly port: PortLike,
    private readonly onEvent: (ev: Msg) => void,
    private readonly onClose: (link: Link, reason: string | undefined) => void,
    reasonOf: (p: PortLike) => string | undefined,
  ) {
    port.onMessage.addListener((m) => this.receive(m));
    port.onDisconnect.addListener((p) => this.close(reasonOf(p)));
  }

  private waiter(timeoutMs: number, resolve: (m: Msg) => void, reject: (e: Error) => void, drop: (w: Waiter) => void): Waiter {
    const w: Waiter = {
      resolve,
      reject,
      timer: setTimeout(() => {
        drop(w);
        reject(new LinkError("timeout"));
        this.disconnect();
      }, timeoutMs),
    };
    return w;
  }

  private receive(raw: unknown): void {
    this.received = true;
    if (typeof raw !== "object" || raw === null) return;
    const m = raw as Msg;
    if (m.t === "host_error") {
      this.hostError = typeof m.code === "string" ? m.code : "app_unavailable";
      return;
    }
    if (m.t === "box") {
      // Uma de cada vez, na ordem de chegada (o contador do canal exige). / One at a time, in arrival order (the channel counter requires it).
      this.inbox = this.inbox.then(() => this.receiveBox(m));
      return;
    }
    const w = this.plainWaiters.shift();
    if (w) {
      clearTimeout(w.timer);
      w.resolve(m);
    }
  }

  private async receiveBox(m: Msg): Promise<void> {
    if (!this.channel || typeof m.c !== "string") return this.disconnect();
    let obj: Msg;
    try {
      const plain = await (await this.channel).open(fromHex(m.c));
      obj = JSON.parse(new TextDecoder().decode(plain)) as Msg;
    } catch {
      // Adulterada ou fora de ordem: derruba a conexão. / Tampered or out of order: drop the connection.
      return this.disconnect();
    }
    if ("ev" in obj) return this.onEvent(obj);
    const w = typeof obj.rid === "number" ? this.boxWaiters.get(obj.rid) : undefined;
    if (w) {
      this.boxWaiters.delete(obj.rid as number);
      clearTimeout(w.timer);
      w.resolve(obj);
    }
  }

  /** Mensagem em claro (antes do canal); espera a resposta. / Plain message (before the channel); awaits the reply. */
  plain(msg: Msg): Promise<Msg> {
    if (this.closed) return Promise.reject(new LinkError("closed"));
    return new Promise((resolve, reject) => {
      const w = this.waiter(REPLY_TIMEOUT_MS, resolve, reject, (x) => (this.plainWaiters = this.plainWaiters.filter((y) => y !== x)));
      this.plainWaiters.push(w);
      this.port.postMessage(msg);
    });
  }

  /** Pedido criptografado; espera a resposta com o mesmo `rid`. / Encrypted request; awaits the reply with the same `rid`. */
  call(op: string): Promise<Msg> {
    const channel = this.channel;
    if (this.closed || !channel) return Promise.reject(new LinkError("closed"));
    const rid = this.nextRid++;
    return new Promise((resolve, reject) => {
      const w = this.waiter(REPLY_TIMEOUT_MS, resolve, reject, () => this.boxWaiters.delete(rid));
      this.boxWaiters.set(rid, w);
      const bytes = new TextEncoder().encode(JSON.stringify({ op, rid }));
      this.outbox = this.outbox.then(async () => {
        const sealed = await (await channel).seal(bytes);
        if (!this.closed) this.port.postMessage({ t: "box", c: toHex(sealed) });
      });
    });
  }

  disconnect(): void {
    if (this.closed) return;
    try {
      this.port.disconnect();
    } catch {
      /* já fechada / already closed */
    }
    this.close(undefined);
  }

  private close(reason: string | undefined): void {
    if (this.closed) return;
    this.closed = true;
    const err = new LinkError(this.hostError ?? reason ?? "closed");
    for (const w of [...this.plainWaiters, ...this.boxWaiters.values()]) {
      clearTimeout(w.timer);
      w.reject(err);
    }
    this.plainWaiters = [];
    this.boxWaiters.clear();
    this.onClose(this, reason);
  }
}

/** Erros do app que encerram a tentativa de pareamento. / App errors that end the pairing attempt. */
const PAIRING_ENDERS = new Set(["locked", "busy", "slow_down", "too_many_pairings", "expired", "too_many_attempts", "no_pairing", "out_of_order"]);

export class Controller {
  private link: Link | null = null;
  private stored: StoredPairing | null | undefined;
  private queue: Promise<unknown> = Promise.resolve();
  private view: View = { health: "unknown", paired: false, connected: false, locked: null, username: null, pairing: "idle", error: null };

  constructor(private readonly deps: Deps) {}

  getView(): View {
    return { ...this.view };
  }

  private update(patch: Partial<View>): void {
    Object.assign(this.view, patch);
    this.deps.onChange(this.getView());
  }

  /** Uma operação de cada vez (popup e inicialização podem chamar juntos). / One operation at a time. */
  private serial<T>(fn: () => Promise<T>): Promise<T> {
    const next = this.queue.then(fn, fn);
    this.queue = next.catch(() => undefined);
    return next;
  }

  private async loadStored(): Promise<StoredPairing | null> {
    if (this.stored === undefined) this.stored = await this.deps.store.load();
    return this.stored;
  }

  private healthFromClose(link: Link, reason: string | undefined): Health {
    if (link.hostError === "untrusted_app") return "untrusted_app";
    if (link.hostError === "bad_caller") return "unknown_extension";
    if (link.hostError) return "app_unavailable";
    const r = (reason ?? "").toLowerCase();
    if (r.includes("not found") || r.includes("no such native application")) return "no_host";
    if (r.includes("forbidden")) return "unknown_extension";
    return "app_unavailable";
  }

  private ensureLink(): Link {
    if (this.link && !this.link.closed) return this.link;
    const link = new Link(
      this.deps.connect(),
      (ev) => void this.onEvent(ev),
      (l, reason) => {
        if (this.link !== l) return;
        this.link = null;
        this.update({ health: this.healthFromClose(l, reason), connected: false, locked: null, username: null, pairing: "idle" });
      },
      (p) => this.deps.disconnectReason(p),
    );
    this.link = link;
    return link;
  }

  private applyStatus(s: Msg): void {
    this.update({ locked: s.locked === true, username: typeof s.username === "string" ? s.username : null });
  }

  /** Esquece o pareamento. A conexão é solta ANTES de cair, para a queda (pedida pelo app ou por
   *  nós) não aparecer como falha. / Forgets the pairing. The connection is released BEFORE it
   *  drops, so the drop (asked by the app or by us) does not show up as a failure. */
  private async forgetPairing(error: string | null): Promise<void> {
    const link = this.link;
    this.link = null;
    link?.disconnect();
    await this.deps.store.clear();
    this.stored = null;
    this.update({ health: "ok", paired: false, connected: false, locked: null, username: null, pairing: "idle", error });
  }

  private async onEvent(ev: Msg): Promise<void> {
    if (ev.ev === "status") this.applyStatus(ev);
    if (ev.ev === "unpaired") await this.forgetPairing("pairing_removed");
  }

  /** Erro em claro vindo do app. / Plain error from the app. */
  private appError(r: Msg): void {
    const code = typeof r.code === "string" ? r.code : "unexpected";
    if (code === "unknown_extension") return this.update({ health: "unknown_extension", error: null, pairing: "idle" });
    this.update({ error: code, ...(PAIRING_ENDERS.has(code) ? { pairing: "idle" as const } : {}) });
  }

  private async authenticate(link: Link, stored: StoredPairing): Promise<void> {
    const nonce = randomBytes(32);
    const r = await link.plain({ t: "auth", id: toHex(stored.id), n: toHex(nonce) });
    if (r.t === "auth" && typeof r.n === "string") {
      link.channel = Channel.create(stored.key, nonce, fromHex(r.n));
      await link.channel;
      this.update({ health: "ok", connected: true, error: null });
      this.applyStatus(await link.call("status"));
    } else if (r.code === "unknown_pairing") {
      // O app não conhece mais este pareamento (foi removido lá). / The app no longer knows this pairing.
      await this.forgetPairing("pairing_removed");
    } else {
      this.appError(r);
    }
  }

  /** Conecta (abrindo o app se preciso), autentica e lê o estado. / Connects (starting the app if needed), authenticates and reads the status. */
  refresh(): Promise<View> {
    return this.serial(async () => {
      const stored = await this.loadStored();
      this.update({ paired: !!stored });
      try {
        const link = this.ensureLink();
        if (stored) {
          if (!link.channel) await this.authenticate(link, stored);
          else this.applyStatus(await link.call("status"));
        } else {
          const r = await link.plain({ t: "hello" });
          if (r.t === "hello") this.update({ health: "ok" });
          else this.appError(r);
        }
      } catch {
        /* a queda já atualizou a tela / the drop already updated the view */
      }
      return this.getView();
    });
  }

  /** Pede ao app para mostrar um código. / Asks the app to show a code. */
  startPairing(): Promise<View> {
    return this.serial(async () => {
      if (await this.loadStored()) return this.getView();
      this.update({ error: null });
      try {
        const r = await this.ensureLink().plain({ t: "pair_start", browser: this.deps.browserName });
        if (r.t === "pair_started") this.update({ health: "ok", pairing: "waiting_code" });
        else this.appError(r);
      } catch {
        /* idem */
      }
      return this.getView();
    });
  }

  /** O usuário digitou o código mostrado no app. / The user typed the code shown in the app. */
  submitCode(code: string): Promise<View> {
    return this.serial(async () => {
      const link = this.link;
      if (!link || link.closed || this.view.pairing !== "waiting_code") {
        this.update({ pairing: "idle", error: "no_pairing" });
        return this.getView();
      }
      const pake = await this.deps.pake();
      try {
        let request: Uint8Array;
        try {
          request = pake.loginStart(code);
        } catch (e) {
          if (e instanceof MalformedCodeError) {
            this.update({ error: "malformed_code" });
            return this.getView();
          }
          throw e;
        }
        const r1 = await link.plain({ t: "pair_login1", req: toHex(request) });
        if (r1.t !== "pair_login1" || typeof r1.resp !== "string") {
          this.appError(r1);
          return this.getView();
        }
        const finalization = pake.loginFinish(fromHex(r1.resp));
        if (!finalization) {
          // Código errado: o app já contou a tentativa. / Wrong code: the app already counted the attempt.
          this.update({ error: "bad_code" });
          return this.getView();
        }
        const r2 = await link.plain({ t: "pair_login2", fin: toHex(finalization) });
        if (r2.t !== "paired" || typeof r2.id !== "string") {
          this.appError(r2);
          return this.getView();
        }
        const id = fromHex(r2.id);
        const raw = pake.pairingKey(id);
        const key = await importPairingKey(raw);
        raw.fill(0);
        const stored = { id, key };
        await this.deps.store.save(stored);
        this.stored = stored;
        this.update({ paired: true, pairing: "idle", error: null });
        await this.authenticate(link, stored);
      } catch {
        /* idem */
      } finally {
        pake.reset();
      }
      return this.getView();
    });
  }

  /** Desiste do pareamento em andamento. / Gives up the pairing in progress. */
  cancelPairing(): Promise<View> {
    return this.serial(async () => {
      if (this.view.pairing === "waiting_code" && this.link && !this.link.closed) {
        await this.link.plain({ t: "pair_cancel" }).catch(() => undefined);
      }
      this.update({ pairing: "idle", error: null });
      return this.getView();
    });
  }

  /** Mostra a janela do app (para desbloquear). / Shows the app window (to unlock). */
  openApp(): Promise<View> {
    return this.serial(async () => {
      if (this.link?.channel) await this.link.call("open_app").catch(() => undefined);
      return this.getView();
    });
  }

  /** Desfaz o pareamento dos dois lados. / Undoes the pairing on both sides. */
  unpair(): Promise<View> {
    return this.serial(async () => {
      const link = this.link;
      // O app apaga o pareamento e fecha a conexão; ignoramos essa queda. / The app deletes the pairing and closes the connection; we ignore that drop.
      this.link = null;
      if (link?.channel) await link.call("unpair").catch(() => undefined);
      link?.disconnect();
      await this.forgetPairing(null);
      return this.getView();
    });
  }
}
