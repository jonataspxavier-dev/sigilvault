// Segundo plano da extensão (service worker no Chrome/Brave/Opera, página de eventos no Firefox).
// Mantém a conexão com o app, troca o ícone da barra e atende o popup.
// Extension background (service worker in Chrome/Brave/Opera, event page in Firefox).
// Keeps the connection with the app, switches the toolbar icon and serves the popup.

import { Controller, iconState, type View } from "./controller";
import { HOST_NAME, type MessageSender, ext } from "./ext";
import { t } from "./i18n";
import { indexedDbStore } from "./keystore";
import { type Pake, loadPake } from "./pake";
import { Router } from "./router";

const api = ext();

/** Nome do navegador, só para o app mostrar. / Browser name, only for the app to show. */
function browserName(): string {
  const ua = navigator.userAgent;
  if (/Firefox\//.test(ua)) return "Firefox";
  if (/OPR\/|Opera/.test(ua)) return "Opera";
  if ((navigator as Navigator & { brave?: unknown }).brave) return "Brave";
  return "Chrome";
}

let pake: Promise<Pake> | null = null;
function getPake(): Promise<Pake> {
  pake ??= fetch(api.runtime.getURL("pairing.wasm"))
    .then((r) => r.arrayBuffer())
    .then(loadPake)
    .catch((e: unknown) => {
      pake = null;
      throw e;
    });
  return pake;
}

const ICON_SIZES = ["16", "32", "48", "128"];

async function showIcon(view: View): Promise<void> {
  const state = iconState(view);
  const path = Object.fromEntries(ICON_SIZES.map((s) => [s, `icons/${state}/${s}x${s}.png`]));
  await api.action.setIcon({ path }).catch(() => undefined);
  await api.action.setTitle({ title: t(state === "off" ? "title_off" : state === "locked" ? "title_locked" : "title_unlocked") }).catch(() => undefined);
}

const controller = new Controller({
  connect: () => api.runtime.connectNative(HOST_NAME),
  // Chrome/Brave/Opera: runtime.lastError; Firefox: port.error.
  disconnectReason: (port) => port.error?.message ?? api.runtime.lastError?.message ?? undefined,
  pake: getPake,
  store: indexedDbStore,
  browserName: browserName(),
  onChange: (view) => {
    void showIcon(view);
    // Atualiza o popup, se estiver aberto (sem popup, ninguém ouve: erro ignorado).
    // Updates the popup if open (with no popup nobody listens: error ignored).
    api.runtime.sendMessage({ type: "view", view }).catch(() => undefined);
  },
});

// Fase 5: páginas e balões. / Phase 5: pages and balloons.
const router = new Router({
  request: (op, extra, timeoutMs) => controller.request(op, extra, timeoutMs),
  sendToFrame: (tabId, frameId, msg) => api.tabs.sendMessage(tabId, msg, { frameId }),
  openApp: () => controller.openApp(),
  uiUrl: api.runtime.getURL("ui/frame.html"),
});
api.tabs.onRemoved.addListener((tabId) => router.tabClosed(tabId));

/** Mensagem do script de uma página (não de uma página da extensão). / Message from a page script (not from an extension page). */
function fromPageScript(sender: MessageSender): boolean {
  return sender.id === api.runtime.id && !!sender.tab && !(sender.url ?? "").startsWith(api.runtime.getURL(""));
}

/** Mensagem do balão (página da extensão dentro de uma aba). / Message from the balloon (extension page inside a tab). */
function fromBalloon(sender: MessageSender): boolean {
  return sender.id === api.runtime.id && !!sender.tab && (sender.url ?? "").startsWith(api.runtime.getURL("ui/frame.html"));
}

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (typeof msg !== "object" || msg === null) return;
  const m = msg as Record<string, unknown>;
  const route = fromBalloon(sender) ? router.fromUi(m, sender) : fromPageScript(sender) ? router.fromContent(m, sender) : null;
  if (!route) return;
  route.then(sendResponse, () => sendResponse({ error: "internal" }));
  return true;
});

/** Só o nosso popup pode dar ordens (nenhuma página e nenhuma outra extensão).
 *  Only our popup may give orders (no web page and no other extension). */
function fromOurPopup(sender: MessageSender): boolean {
  return sender.id === api.runtime.id && !sender.tab && (sender.url ?? "").startsWith(api.runtime.getURL("popup/"));
}

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (!fromOurPopup(sender) || typeof msg !== "object" || msg === null) return;
  const m = msg as { cmd?: unknown; code?: unknown };
  const actions: Record<string, () => Promise<View>> = {
    view: () => controller.refresh(),
    pair: () => controller.startPairing(),
    code: () => controller.submitCode(typeof m.code === "string" ? m.code.slice(0, 32) : ""),
    cancel: () => controller.cancelPairing(),
    open: () => controller.openApp(),
    unpair: () => controller.unpair(),
  };
  const action = typeof m.cmd === "string" ? actions[m.cmd] : undefined;
  if (!action) return;
  action().then(sendResponse, () => sendResponse(controller.getView()));
  return true; // resposta assíncrona / async response
});

// Ao iniciar: só conecta sozinho se já estiver pareado (antes disso, nada acontece sem um clique).
// On start: only connects by itself if already paired (before that, nothing happens without a click).
async function start(): Promise<void> {
  await showIcon(controller.getView());
  if (await indexedDbStore.load().catch(() => null)) await controller.refresh();
}

api.runtime.onStartup.addListener(() => void start());
api.runtime.onInstalled.addListener(() => void start());
void start();
