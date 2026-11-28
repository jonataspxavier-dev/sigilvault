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
//        cd app; npx tauri dev
//   3. SIGILVAULT_E2E=1 npm test
// NUNCA rode contra o seu cofre real: o teste cria e tranca um cofre.
// NEVER run against your real vault: the test creates and locks a vault.


import { connect as netConnect } from "node:net";
import { describe, expect, it } from "vitest";

import { importPairingKey } from "./channel";
import { OURS, PASSWORD, USER, bridgeAddress, controller, ensureUnlocked, invoke, pair, waitFor, wrong } from "./e2e-helpers";
import { randomBytes } from "./hex";
import { memoryStore } from "./keystore";

describe("ponta a ponta com o app / end to end with the app", () => {
  it("pareia, recebe avisos, reconecta e perde acesso ao ser removida / pairs, gets notices, reconnects and loses access when removed", async () => {
    await ensureUnlocked();
    // O app exige 3 s entre pedidos de pareamento. / The app requires 3 s between pairing requests.
    await new Promise((r) => setTimeout(r, 3500));
    // Começa sem pareamentos de rodadas anteriores. / Starts without pairings from earlier runs.
    for (const p of await invoke<{ id: string }[]>("list_pairings")) await invoke("remove_pairing", { id: p.id });
    const store = memoryStore();
    const c = controller(store);

    let v = await c.refresh();
    expect(v.health).toBe("ok");
    expect(v.paired).toBe(false);

    const started = await c.startPairing();
  expect(started, JSON.stringify(started)).toMatchObject({ pairing: "waiting_code" });
    const pending = await invoke<{ code: string; browser: string; attemptsLeft: number }>("pairing_pending");
    expect(pending.browser).toBe("Teste");
    expect(pending.attemptsLeft).toBe(3);

    // Código errado: recusado, e o app desconta a tentativa. / Wrong code: refused, and the app counts the attempt.
    v = await c.submitCode(wrong(pending.code));
    expect(v.error).toBe("bad_code");
    expect(v.paired).toBe(false);
    expect((await invoke<{ attemptsLeft: number }>("pairing_pending")).attemptsLeft).toBe(2);

    v = await c.submitCode(pending.code.toLowerCase());
    expect(v).toMatchObject({ paired: true, connected: true, locked: false, username: USER, error: null });
    expect(await invoke("pairing_pending")).toBeNull();

    const list = await invoke<{ id: string; browser: string; connected: boolean }[]>("list_pairings");
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ browser: "Teste", connected: true });

    // Aviso ao vivo: trancar e abrir no app. / Live notice: lock and unlock in the app.
    await invoke("lock");
    await waitFor(() => c.getView().locked === true);
    await waitFor(async () => !(await invoke<{ unlocked: boolean }>("app_state")).unlocked);
    await invoke("unlock", { password: PASSWORD });
    await waitFor(() => c.getView().locked === false);

    // "Reinício" da extensão: novo controlador com a chave guardada. / Extension "restart": new controller with the stored key.
    const c2 = controller(store);
    expect(await c2.refresh()).toMatchObject({ paired: true, connected: true, locked: false });

    // Remover no app: as duas conexões perdem o acesso e a chave some. / Remove in the app: both connections lose access and the key is gone.
    await invoke("remove_pairing", { id: list[0]!.id });
    await waitFor(() => !c.getView().paired && !c2.getView().paired);
    expect(await store.load()).toBeNull();
    expect(c.getView()).toMatchObject({ error: "pairing_removed", health: "ok" });
  });

  it("recusa uma extensão com outro ID / refuses an extension with another ID", async () => {
    const v = await controller(memoryStore(), "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/").refresh();
    expect(v.health).toBe("unknown_extension");
  });

  it("chave errada derruba a conexão / a wrong key drops the connection", async () => {
    const store = memoryStore();
    expect(await pair(controller(store))).toMatchObject({ paired: true, connected: true });
    const real = (await store.load())!;
    // Mesmo id, chave inventada: o app não consegue abrir a mensagem e fecha. / Same id, made-up key.
    const fake = memoryStore();
    await fake.save({ id: real.id, key: await importPairingKey(randomBytes(32)) });
    const v = await controller(fake).refresh();
    expect(v.connected).toBe(false);
    expect(v.locked).toBeNull();
    // A extensão verdadeira continua funcionando. / The real extension keeps working.
    expect(await controller(store).refresh()).toMatchObject({ connected: true });
    const [entry] = await invoke<{ id: string }[]>("list_pairings");
    await invoke("remove_pairing", { id: entry!.id });
  });

  it("id de pareamento desconhecido é esquecido / unknown pairing id is forgotten", async () => {
    const store = memoryStore();
    await store.save({ id: randomBytes(16), key: await importPairingKey(randomBytes(32)) });
    const v = await controller(store).refresh();
    expect(v).toMatchObject({ paired: false, error: "pairing_removed" });
    expect(await store.load()).toBeNull();
  });

  it("um programa que não é o host não entra no pipe / a program that is not the host cannot use the pipe", async () => {
    const sock = netConnect(bridgeAddress());
    const body = Buffer.from(JSON.stringify({ t: "host", v: 1, caller: OURS }));
    const head = Buffer.alloc(4);
    head.writeUInt32LE(body.length);
    const got: Buffer[] = [];
    const closed = new Promise<void>((resolve) => {
      sock.on("data", (d) => got.push(d));
      sock.on("close", () => resolve());
      sock.on("error", () => resolve());
    });
    sock.on("connect", () => sock.write(Buffer.concat([head, body])));
    await closed;
    // O app fecha sem responder nada. / The app closes without answering anything.
    expect(Buffer.concat(got).length).toBe(0);
  });
});
