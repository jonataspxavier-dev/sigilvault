// Fase 5, ponta a ponta com o app real: consultar logins, preencher (com a confirmação do app),
// salvar, histórico, gerador, registro e limites. Pré-requisitos em `e2e-helpers.ts`.
// Phase 5, end to end with the real app: query logins, fill (with the app's confirmation), save,
// history, generator, log and limits. Prerequisites in `e2e-helpers.ts`.

import { writeFileSync } from "node:fs";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import type { Controller } from "./controller";
import { controller, ensureUnlocked, invoke, pair, PASSWORD } from "./e2e-helpers";
import { memoryStore } from "./keystore";

const SITE = "https://e2e.example";
const at = (origin: string, top = origin) => ({ origin, top });

/** Resposta que o app dará ao próximo diálogo. / Answer the app will give to the next dialog. */
function answer(a: "always" | "once" | "cancel"): void {
  const control = process.env.SIGILVAULT_TEST_DIALOG;
  if (!control) throw new Error("defina SIGILVAULT_TEST_DIALOG / set SIGILVAULT_TEST_DIALOG");
  writeFileSync(control, a);
}

let c: Controller;
let id = "";

beforeAll(async () => {
  await ensureUnlocked();
  for (const p of await invoke<{ id: string }[]>("list_pairings")) await invoke("remove_pairing", { id: p.id });
  // Começa sem itens deste site. / Starts without items for this site.
  for (const i of await invoke<{ id: string; url: string }[]>("list_items", { query: "e2e.example" })) await invoke("delete_item", { id: i.id });
  await new Promise((r) => setTimeout(r, 3500));
  c = controller(memoryStore());
  expect(await pair(c)).toMatchObject({ paired: true, connected: true });
});

afterAll(async () => {
  for (const p of await invoke<{ id: string }[]>("list_pairings").catch(() => [])) await invoke("remove_pairing", { id: p.id }).catch(() => undefined);
});

describe("navegador com o app real / browser with the real app", () => {
  it("salva e só oferece o login na origem certa / saves and only offers the login on the right origin", async () => {
    expect(await c.request("logins", at(SITE))).toMatchObject({ locked: false, logins: [], site: "e2e.example", secure: true });
    const saved = await c.request("save", { ...at(SITE), username: "ana", password: "senha-1" });
    expect(saved).toMatchObject({ saved: true, outcome: "added" });
    id = saved.id as string;

    expect(((await c.request("logins", at(`${SITE}/entrar`))).logins as unknown[]).length).toBe(1);
    for (const other of ["http://e2e.example", "https://login.e2e.example", "https://e2e.example.golpe.net", "https://e2e.example:8443"]) {
      expect((await c.request("logins", at(other))).logins, other).toEqual([]);
    }
    // Origem sem protocolo (não veio do navegador): recusada. / Origin without scheme (not from the browser): refused.
    expect(await c.request("logins", at("e2e.example"))).toEqual({ error: "bad_origin", rid: expect.any(Number) });
  });

  it("sabe se o login enviado é novo, igual ou mudou / knows whether the submitted login is new, same or changed", async () => {
    expect(await c.request("check_save", { ...at(SITE), username: "ANA ", password: "senha-1" })).toMatchObject({ status: "same" });
    expect(await c.request("check_save", { ...at(SITE), username: "ana", password: "senha-2" })).toMatchObject({ status: "changed", item: { id, title: "e2e.example" } });
    expect(await c.request("check_save", { ...at(SITE), username: "bia", password: "x" })).toMatchObject({ status: "new" });
    // A resposta nunca traz a senha salva. / The reply never carries the saved password.
    expect(JSON.stringify(await c.request("check_save", { ...at(SITE), username: "ana", password: "outra" }))).not.toContain("senha-1");
  });

  it("preencher pede confirmação; 'sempre' fica lembrado; iframe de outro site pergunta sempre / fill asks; 'always' is remembered; other site's iframe always asks", async () => {
    answer("cancel");
    expect(await c.request("fill", { ...at(SITE), id })).toMatchObject({ error: "denied" });
    answer("once");
    expect(await c.request("fill", { ...at(SITE), id })).toMatchObject({ username: "ana", password: "senha-1" });
    // "Só desta vez" não fica lembrado. / "Just this once" is not remembered.
    answer("cancel");
    expect(await c.request("fill", { ...at(SITE), id })).toMatchObject({ error: "denied" });
    answer("always");
    expect(await c.request("fill", { ...at(SITE), id })).toMatchObject({ password: "senha-1" });
    answer("cancel");
    expect(await c.request("fill", { ...at(SITE), id })).toMatchObject({ password: "senha-1" });
    // Mesmo site liberado, mas dentro de iframe de outro site: pergunta. / Allowed site, but inside another site's iframe: asks.
    expect(await c.request("fill", { ...at(SITE, "https://outro.example"), id })).toMatchObject({ error: "denied" });
    // Login de outro site: nunca. / Another site's login: never.
    expect(await c.request("fill", { ...at("https://golpe.example"), id })).toMatchObject({ error: "not_allowed" });

    const [pairing] = await invoke<{ sites: string[] }[]>("list_pairings");
    expect(pairing!.sites).toEqual([SITE]);
    const log = await invoke<{ origin: string; username: string }[]>("fill_log");
    expect(log.length).toBeGreaterThanOrEqual(3);
    expect(log[0]).toMatchObject({ origin: SITE, username: "ana" });
    expect(JSON.stringify(log)).not.toContain("senha-1");
  });

  it("atualizar guarda a senha antiga no histórico / updating keeps the old password in history", async () => {
    expect(await c.request("save", { ...at(SITE), username: "ana", password: "senha-2", replace: id })).toMatchObject({ saved: true, outcome: "updated" });
    const history = await invoke<{ password: string }[]>("reveal_history", { id });
    expect(history.map((h) => h.password)).toEqual(["senha-1"]);
    // Trocar um item usando outra origem: recusado. / Replacing an item from another origin: refused.
    expect(await c.request("save", { ...at("https://golpe.example"), username: "x", password: "y", replace: id })).toMatchObject({ error: "not_allowed" });
  });

  it("gerador com preferências salvas / generator with saved preferences", async () => {
    const prefs = { length: 31, lowercase: true, uppercase: true, digits: true, symbols: false, excludeAmbiguous: true };
    expect(await c.request("save_generator", { options: prefs })).toMatchObject({ options: prefs });
    const g = await c.request("generate");
    expect((g.password as string).length).toBe(31);
    expect(g.password).toMatch(/^[A-Za-z0-9]+$/);
    expect(g.entropyBits).toBeGreaterThan(150);
    expect(await c.request("get_generator")).toMatchObject({ options: prefs });
    expect(await c.request("strength", { password: "123456", username: "" })).toMatchObject({ score: 0 });
  });

  it("cofre trancado e limite de preenchimentos / locked vault and fill limit", async () => {
    answer("always");
    let limited = false;
    for (let n = 0; n < 25 && !limited; n++) limited = (await c.request("fill", { ...at(SITE), id })).error === "rate_limited";
    expect(limited).toBe(true);

    await invoke("lock");
    await new Promise((r) => setTimeout(r, 1500));
    expect(await c.request("logins", at(SITE))).toMatchObject({ locked: true });
    await invoke("unlock", { password: PASSWORD });
  });
});
