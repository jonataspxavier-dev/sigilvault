// APIs do navegador usadas pela extensão, com tipos mínimos escritos à mão (sem dependências).
// Firefox expõe `browser` (com promessas); Chrome, Brave e Opera expõem `chrome`.
// Browser APIs used by the extension, with minimal hand-written types (no dependencies).
// Firefox exposes `browser` (with promises); Chrome, Brave and Opera expose `chrome`.

export interface EventOf<T extends (...args: never[]) => unknown> {
  addListener(cb: T): void;
  removeListener(cb: T): void;
}

/** Porta do Native Messaging. / Native Messaging port. */
export interface PortLike {
  postMessage(msg: unknown): void;
  disconnect(): void;
  onMessage: EventOf<(msg: unknown) => void>;
  onDisconnect: EventOf<(port: PortLike) => void>;
  /** Firefox: motivo da desconexão. / Firefox: disconnect reason. */
  error?: { message?: string } | null;
}

export interface MessageSender {
  id?: string;
  url?: string;
  /** Origem do quadro, dada pelo navegador (Chrome/Brave/Opera). / Frame origin, given by the browser. */
  origin?: string;
  frameId?: number;
  tab?: { id?: number; url?: string };
}

interface ExtApi {
  runtime: {
    id: string;
    lastError?: { message?: string } | null;
    connectNative(application: string): PortLike;
    sendMessage(msg: unknown): Promise<unknown>;
    getURL(path: string): string;
    onMessage: EventOf<(msg: unknown, sender: MessageSender, sendResponse: (r?: unknown) => void) => boolean | void>;
    onStartup: EventOf<() => void>;
    onInstalled: EventOf<() => void>;
  };
  tabs: {
    sendMessage(tabId: number, msg: unknown, options?: { frameId?: number }): Promise<unknown>;
    onRemoved: EventOf<(tabId: number) => void>;
  };
  action: {
    setIcon(details: { path: Record<string, string> }): Promise<void>;
    setTitle(details: { title: string }): Promise<void>;
  };
}

declare const chrome: ExtApi | undefined;
declare const browser: ExtApi | undefined;

/** API do navegador atual. / Current browser's API. */
export function ext(): ExtApi {
  const api = typeof browser !== "undefined" ? browser : typeof chrome !== "undefined" ? chrome : undefined;
  if (!api) throw new Error("APIs de extensão indisponíveis / extension APIs unavailable");
  return api;
}

/** Nome do host registrado no Windows (igual a `sigilvault_ipc::HOST_NAME`). / Host name registered on Windows. */
export const HOST_NAME = "app.sigilvault.host";
