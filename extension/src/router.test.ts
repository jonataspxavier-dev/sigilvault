import { describe, expect, it } from "vitest";

import type { MessageSender } from "./ext";
import { Router, senderOrigin } from "./router";

type Msg = Record<string, unknown>;
const UI = "chrome-extension://id/ui/frame.html";
const TOKEN = "0123456789abcdef0123456789abcdef";

function setup(answers: Record<string, Msg> = {}) {
  const calls: { op: string; extra: Msg }[] = [];
  const toPage: { tabId: number; frameId: number; msg: Msg }[] = [];
  const router = new Router({
    uiUrl: UI,
    request: async (op, extra = {}) => {
      calls.push({ op, extra });
      return answers[op] ?? {};
    },
    sendToFrame: async (tabId, frameId, msg) => {
      toPage.push({ tabId, frameId, msg });
      if (msg.cmd === "bg_fill" || msg.cmd === "bg_fill_new") return { filled: 2 };
      if (msg.cmd === "bg_read") return { username: "ana", password: "digitada" };
      return {};
    },
    openApp: async () => undefined,
  });
  return { router, calls, toPage };
}

const page = (frameId = 0, url = "https://banco.example/login", top = "https://banco.example/"): MessageSender => ({
  id: "ext",
  url,
  frameId,
  tab: { id: 7, url: top },
});
const ui = (frameId = 9, tabId = 7): MessageSender => ({ id: "ext", url: `${UI}#${TOKEN}`, frameId, tab: { id: tabId } });

describe("roteador / router", () => {
  it("só aceita origens web / only accepts web origins", () => {
    expect(senderOrigin({ url: "https://a.example/x" })).toBe("https://a.example");
    expect(senderOrigin({ origin: "null", url: "about:blank" })).toBeNull();
    expect(senderOrigin({ url: "file:///C:/x.html" })).toBeNull();
  });

  it("preenche com a origem do navegador e entrega a senha só à página / fills with the browser origin and hands the password only to the page", async () => {
    const { router, calls, toPage } = setup({ fill: { username: "ana", password: "S3nh@" } });
    expect(await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "login" }, page(3, "https://banco.example/frame"))).toEqual({ ok: true });
    expect(await router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui())).toMatchObject({ kind: "login", site: "banco.example" });
    const res = await router.fromUi({ cmd: "ui_fill", token: TOKEN, id: "abc" }, ui());
    // O balão não recebe a senha. / The balloon does not get the password.
    expect(res).toEqual({ ok: true, filled: 2 });
    expect(JSON.stringify(res)).not.toContain("S3nh@");
    expect(calls[0]).toEqual({ op: "fill", extra: { id: "abc", origin: "https://banco.example", top: "https://banco.example" } });
    expect(toPage.at(-1)).toMatchObject({ tabId: 7, frameId: 3, msg: { cmd: "bg_fill", password: "S3nh@" } });
  });

  it("recusa balões impostores / refuses impostor balloons", async () => {
    const { router } = setup();
    await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "login" }, page());
    // Antes do hello, nada. / Before hello, nothing.
    expect(await router.fromUi({ cmd: "ui_logins", token: TOKEN }, ui())).toEqual({ error: "forbidden" });
    await router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui(9));
    // Outro quadro, outra aba, outra página ou token desconhecido. / Other frame, tab, page or unknown token.
    expect(await router.fromUi({ cmd: "ui_logins", token: TOKEN }, ui(10))).toEqual({ error: "forbidden" });
    expect(await router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui(10))).toEqual({ error: "forbidden" });
    expect(await router.fromUi({ cmd: "ui_logins", token: TOKEN }, ui(9, 8))).toEqual({ error: "forbidden" });
    expect(await router.fromUi({ cmd: "ui_logins", token: TOKEN }, { ...ui(9), url: "https://golpe.example/#x" })).toEqual({ error: "forbidden" });
    expect(await router.fromUi({ cmd: "ui_logins", token: "f".repeat(32) }, ui(9))).toEqual({ error: "forbidden" });
    expect(await router.fromUi({ cmd: "ui_logins", token: TOKEN }, ui(9))).toEqual({});
  });

  it("recusa registros ruins / refuses bad registrations", async () => {
    const { router } = setup();
    expect(await router.fromContent({ cmd: "cs_register", token: "curto", kind: "login" }, page())).toEqual({ error: "bad_message" });
    expect(await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "login" }, page(0, "about:blank", "about:blank"))).toEqual({ error: "bad_sender" });
    // Aviso de salvar só com algo pendente. / Save prompt only with something pending.
    expect(await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "save" }, page())).toEqual({ error: "no_pending" });
  });

  it("salvar depois do envio: pergunta só se novo/mudou, e o aviso nunca vê a senha / save after submit: asks only if new/changed, and the prompt never sees the password", async () => {
    const same = setup({ check_save: { status: "same" } });
    expect(await same.router.fromContent({ cmd: "cs_submitted", username: "ana", password: "x" }, page())).toEqual({ ok: true, prompt: false });

    const { router, calls, toPage } = setup({ check_save: { status: "changed", site: "banco.example", item: { id: "i1", title: "Banco", username: "ana" } }, save: { saved: true } });
    expect(await router.fromContent({ cmd: "cs_submitted", username: "ana", password: "nova" }, page())).toEqual({ ok: true, prompt: true });
    expect(toPage.at(-1)).toMatchObject({ frameId: 0, msg: { cmd: "bg_show_save" } });
    expect(await router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: true });
    await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "save" }, page());
    const hello = await router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui());
    expect(hello).toEqual({ kind: "save", site: "banco.example", save: { username: "ana", status: "changed", title: "Banco" } });
    expect(JSON.stringify(hello)).not.toContain("nova");
    await router.fromUi({ cmd: "ui_save_decision", token: TOKEN, save: true }, ui());
    expect(calls.at(-1)).toEqual({ op: "save", extra: { origin: "https://banco.example", top: "https://banco.example", username: "ana", password: "nova", replace: "i1" } });
    // Depois de salvar, a senha some. / After saving, the password is gone.
    expect(await router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: false });

    // Cofre trancado: o pedido continua para depois do desbloqueio. / Locked vault: the request stays until unlock.
    const locked = setup({ check_save: { status: "new" }, save: { locked: true } });
    await locked.router.fromContent({ cmd: "cs_submitted", username: "a", password: "b" }, page());
    await locked.router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "save" }, page());
    await locked.router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui());
    expect(await locked.router.fromUi({ cmd: "ui_save_decision", token: TOKEN, save: true }, ui())).toEqual({ locked: true });
    expect(await locked.router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: true });
    await locked.router.fromUi({ cmd: "ui_save_decision", token: TOKEN, save: false }, ui());
    expect(await locked.router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: false });
  });

  it("senha pendente expira e some ao fechar a aba / pending password expires and goes away when the tab closes", async () => {
    let now = 0;
    const { router } = setup({ check_save: { status: "new" } });
    (router as unknown as { deps: { now: () => number } }).deps.now = () => now;
    await router.fromContent({ cmd: "cs_submitted", username: "a", password: "b" }, page());
    expect(await router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: true });
    now = 4 * 60_000;
    expect(await router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: false });
    await router.fromContent({ cmd: "cs_submitted", username: "a", password: "b" }, page());
    router.tabClosed(7);
    expect(await router.fromContent({ cmd: "cs_ready" }, page())).toEqual({ pendingSave: false });
  });

  it("usar e salvar: preenche, lê o usuário e salva / use and save: fills, reads the username and saves", async () => {
    const { router, calls } = setup({ save: { saved: true, outcome: "added" } });
    await router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "signup" }, page());
    await router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui());
    expect(await router.fromUi({ cmd: "ui_use_password", token: TOKEN, password: "Gerada!", options: { length: 20 } }, ui())).toEqual({ saved: true, outcome: "added" });
    expect(calls.map((c) => c.op)).toEqual(["save_generator", "save"]);
    expect(calls[1]!.extra).toMatchObject({ username: "ana", password: "Gerada!" });
    // Num formulário de login, não. / Not on a login form.
    const other = setup();
    await other.router.fromContent({ cmd: "cs_register", token: TOKEN, kind: "login" }, page());
    await other.router.fromUi({ cmd: "ui_hello", token: TOKEN }, ui());
    expect(await other.router.fromUi({ cmd: "ui_use_password", token: TOKEN, password: "x" }, ui())).toEqual({ error: "bad_message" });
  });
});
