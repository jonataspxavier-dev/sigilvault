// Com o cofre TRANCADO, nada pode sair: nem código de pareamento, nem lista de contas, nem senha.
// Roda contra o app real (pré-requisitos em `e2e-helpers.ts`).
// With the vault LOCKED, nothing may come out: no pairing code, no account list, no password.
// Runs against the real app (prerequisites in `e2e-helpers.ts`).

import { writeFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { PASSWORD, controller, ensureUnlocked, invoke, pair, waitFor } from "./e2e-helpers";
import { memoryStore } from "./keystore";

const SITE = "https://trancado.example";
const at = { origin: SITE, top: SITE };

describe("cofre trancado não entrega nada / locked vault hands out nothing", () => {
  it("pareamento, consulta, preenchimento e salvar recusados / pairing, query, fill and save refused", async () => {
    // Preparação: um login salvo e um navegador pareado. / Setup: a saved login and a paired browser.
    await ensureUnlocked();
    for (const p of await invoke<{ id: string }[]>("list_pairings")) await invoke("remove_pairing", { id: p.id });
    await new Promise((r) => setTimeout(r, 3500));
    const store = memoryStore();
    const paired = controller(store);
    expect(await pair(paired)).toMatchObject({ paired: true, connected: true });
    const saved = await paired.request("save", { ...at, username: "ana", password: "segredo-trancado" });
    expect(saved).toMatchObject({ saved: true });
    // Até "sempre neste site", para provar que nem a permissão salva abre o cofre.
    // Even "always on this site", to prove that not even a saved permission opens the vault.
    const control = process.env.SIGILVAULT_TEST_DIALOG;
    if (control) writeFileSync(control, "always");
    expect(await paired.request("fill", { ...at, id: saved.id })).toMatchObject({ password: "segredo-trancado" });

    // Tranca. / Lock.
    await invoke("lock");
    await waitFor(async () => !(await invoke<{ unlocked: boolean }>("app_state")).unlocked);

    // Navegador pareado: tudo recusado, e a senha não aparece em nenhuma resposta.
    // Paired browser: everything refused, and the password appears in no reply.
    const replies = [
      await paired.request("logins", at),
      await paired.request("fill", { ...at, id: saved.id }),
      await paired.request("check_save", { ...at, username: "ana", password: "x" }),
      await paired.request("save", { ...at, username: "bia", password: "y" }),
    ];
    for (const r of replies) expect(r).toMatchObject({ locked: true });
    expect(JSON.stringify(replies)).not.toContain("segredo-trancado");

    // Navegador NÃO pareado: nem código de pareamento. / NOT paired browser: not even a pairing code.
    await new Promise((r) => setTimeout(r, 3500));
    const stranger = controller(memoryStore());
    const v = await stranger.startPairing();
    expect(v).toMatchObject({ pairing: "idle", error: "locked" });
    // O app também não tem nenhum código pendente. / The app has no pending code either.
    await invoke("unlock", { password: PASSWORD });
    expect(await invoke("pairing_pending")).toBeNull();

    // Limpeza. / Cleanup.
    for (const p of await invoke<{ id: string }[]>("list_pairings")) await invoke("remove_pairing", { id: p.id });
    await invoke("delete_item", { id: saved.id });
  });
});
