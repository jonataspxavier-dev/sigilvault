// Ajudantes dos testes de ponta a ponta (não é teste: é importado por eles).
// End-to-end test helpers (not a test: imported by them).
//
// Teste de ponta a ponta com o app de verdade: este teste faz o papel do navegador (abre o
// sigilvault-host.exe e fala Native Messaging), usando o mesmo controller.ts da extensão.
// End-to-end test with the real app: this test plays the browser (starts sigilvault-host.exe and
// speaks Native Messaging), using the extension's own controller.ts.
//
// Pré-requisitos / Prerequisites:
//   1. cargo build -p sigilvault-host -p sigilvault-pairing --target ... (o build da extensão faz o wasm)
//   2. App de depuração rodando com uma pasta de teste e a porta de depuração do WebView2:
//      Debug app running with a test folder and the WebView2 debugging port:
//        $env:SIGILVAULT_DATA_DIR = "<pasta temporária>"
//        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333"
//        $env:SIGILVAULT_TEST_DIALOG = "<arquivo de controle>"   # respostas dos diálogos / dialog answers
//        cd app; npx tauri dev
//      O teste de preenchimento escreve "always"/"once"/"cancel" nesse arquivo (mesma variável).
//      The fill test writes "always"/"once"/"cancel" into that file (same variable).
//   3. SIGILVAULT_E2E=1 npm test
// NUNCA rode contra o seu cofre real: o teste cria e tranca um cofre.
// NEVER run against your real vault: the test creates and locks a vault.


import { execSync, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect } from "vitest";

import { Controller, type View } from "./controller";
import type { PortLike } from "./ext";
import type { KeyStore } from "./keystore";
import { type Pake, loadPake } from "./pake";

export const ROOT = join(__dirname, "..", "..");
export const HOST = join(ROOT, "target", "debug", "sigilvault-host.exe");
export const WASM = join(ROOT, "target", "wasm32-unknown-unknown", "release", "sigilvault_pairing.wasm");
export const OURS = "chrome-extension://jclandjcanimgllfngognekhhhnomimf/";
export const USER = "teste-e2e";
export const PASSWORD = "trilho-cavalo-nuvem-farol-sete-mesa";

// ── Porta "Native Messaging" feita com um processo filho / Native Messaging port made with a child process ──

export function hostPort(caller: string): PortLike {
  const child = spawn(HOST, [caller, "--parent-window=0"], { stdio: ["pipe", "pipe", "ignore"] });
  const onMsg: ((m: unknown) => void)[] = [];
  const onDisc: ((p: PortLike) => void)[] = [];
  let buf = Buffer.alloc(0);
  let gone = false;
  const port: PortLike = {
    postMessage(m) {
      const body = Buffer.from(JSON.stringify(m));
      const head = Buffer.alloc(4);
      head.writeUInt32LE(body.length);
      child.stdin.write(Buffer.concat([head, body]));
    },
    disconnect() {
      gone = true;
      child.kill();
    },
    onMessage: { addListener: (cb) => onMsg.push(cb), removeListener: () => undefined },
    onDisconnect: { addListener: (cb) => onDisc.push(cb), removeListener: () => undefined },
    error: null,
  };
  child.stdout.on("data", (d: Buffer) => {
    buf = Buffer.concat([buf, d]);
    while (buf.length >= 4) {
      const len = buf.readUInt32LE(0);
      if (buf.length < 4 + len) break;
      const msg = JSON.parse(buf.subarray(4, 4 + len).toString("utf8")) as unknown;
      buf = buf.subarray(4 + len);
      onMsg.forEach((cb) => cb(msg));
    }
  });
  child.on("exit", () => {
    if (gone) return;
    port.error = { message: "Native host has exited." };
    onDisc.forEach((cb) => cb(port));
  });
  return port;
}

// ── Comandos do app via WebView2 (CDP) / App commands via WebView2 (CDP) ──

export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const targets = (await (await fetch("http://127.0.0.1:9333/json")).json()) as { type: string; webSocketDebuggerUrl: string }[];
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("janela do app não encontrada / app window not found");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r, { once: true }));
  const expression = `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(v => JSON.stringify({ ok: v ?? null }), e => JSON.stringify({ err: e }))`;
  // `lock` destrói a janela: a página some sem responder, e isso conta como sucesso.
  // `lock` destroys the window: the page goes away without answering, which counts as success.
  const reply = await new Promise<{ result?: { result?: { value?: string } } } | null>((resolve) => {
    const timer = setTimeout(() => resolve(null), 10_000);
    const done = (v: { result?: { result?: { value?: string } } } | null) => (clearTimeout(timer), resolve(v));
    ws.addEventListener("message", (ev) => done(JSON.parse(String(ev.data))), { once: true });
    ws.addEventListener("close", () => done(null), { once: true });
    ws.addEventListener("error", () => done(null), { once: true });
    ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
  });
  ws.close();
  const value = reply?.result?.result?.value;
  if (value === undefined) {
    if (cmd === "lock") return null as T;
    throw new Error(`${cmd}: sem resposta / no reply`);
  }
  const out = JSON.parse(value) as { ok?: T; err?: unknown };
  if ("err" in out) throw new Error(`${cmd}: ${JSON.stringify(out.err)}`);
  return out.ok as T;
}

export async function waitFor(cond: () => boolean | Promise<boolean>, ms = 15_000): Promise<void> {
  const end = Date.now() + ms;
  for (;;) {
    try {
      if (await cond()) return;
    } catch {
      /* janela trocando / window switching */
    }
    if (Date.now() > end) throw new Error("tempo esgotado / timed out");
    await new Promise((r) => setTimeout(r, 250));
  }
}

export async function ensureUnlocked(): Promise<void> {
  await waitFor(async () => (await invoke<{ hasVault: boolean }>("app_state")) !== null);
  const s = await invoke<{ hasVault: boolean; unlocked: boolean }>("app_state");
  if (!s.hasVault) await invoke("create_vault", { username: USER, password: PASSWORD });
  else if (!s.unlocked) await invoke("unlock", { password: PASSWORD });
}

let pakeP: Promise<Pake> | null = null;
const pake = () => (pakeP ??= loadPake(readFileSync(WASM)));

export function controller(store: KeyStore, caller = OURS, onChange: (v: View) => void = () => undefined): Controller {
  return new Controller({ connect: () => hostPort(caller), disconnectReason: (p) => p.error?.message, pake, store, browserName: "Teste", onChange });
}

/** Troca um caractere do código por outro válido. / Swaps one code character for another valid one. */
export function wrong(code: string): string {
  return (code[0] === "Z" ? "Y" : "Z") + code.slice(1);
}

export async function pair(c: Controller): Promise<View> {
  // O app exige 3 s entre pedidos de pareamento. / The app requires 3 s between pairing requests.
  await new Promise((r) => setTimeout(r, 3500));
  const started = await c.startPairing();
  expect(started, JSON.stringify(started)).toMatchObject({ pairing: "waiting_code" });
  const pending = await invoke<{ code: string }>("pairing_pending");
  return c.submitCode(pending.code);
}

/** SID do usuário (nome do pipe). / User SID (pipe name). */
export function userSid(): string {
  return /S-1-5-[\d-]+/.exec(execSync(String.raw`C:\Windows\System32\whoami.exe /user`, { encoding: "utf8" }))![0];
}
