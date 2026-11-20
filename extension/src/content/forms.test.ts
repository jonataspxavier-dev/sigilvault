// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";

import { contextFor, fillLogin, fillNewPassword, readCredentials } from "./forms";

// O happy-dom não calcula layout; "visível" aqui = sem `hidden` e sem display:none.
// happy-dom does not compute layout; "visible" here = no `hidden` and no display:none.
const visible = (el: HTMLElement) => !el.closest("[hidden]") && el.style.display !== "none";

const $ = <T extends Element = HTMLInputElement>(sel: string) => document.querySelector(sel) as unknown as T;

beforeEach(() => {
  document.body.replaceChildren();
});

function page(html: string): void {
  // Só em teste (conteúdo fixo). / Tests only (fixed content).
  document.body.innerHTML = html;
}

describe("detecção / detection", () => {
  it("login comum / plain login", () => {
    page(`<form id="entrar"><input name="email" type="email"><input type="password" id="pw"><button>Entrar</button></form>`);
    const ctx = contextFor($("#pw"), visible)!;
    expect(ctx.kind).toBe("login");
    expect(ctx.username).toBe($('input[name="email"]'));
    expect(ctx.passwords).toHaveLength(1);
    // Também a partir do campo de usuário. / Also from the username field.
    expect(contextFor($('input[name="email"]'), visible)?.passwords).toHaveLength(1);
  });

  it("cadastro com confirmação / sign-up with confirmation", () => {
    page(`<form><input name="user"><input type="password" id="a"><input type="password" id="b"><button>Criar conta</button></form>`);
    expect(contextFor($("#a"), visible)?.kind).toBe("signup");
  });

  it("cadastro por autocomplete ou pelo texto / sign-up by autocomplete or text", () => {
    page(`<form><input name="user"><input type="password" id="a" autocomplete="new-password"></form>`);
    expect(contextFor($("#a"), visible)?.kind).toBe("signup");
    page(`<form action="/cadastro"><input name="user"><input type="password" id="a"></form>`);
    expect(contextFor($("#a"), visible)?.kind).toBe("signup");
  });

  it("sem <form>: acha o bloco / without <form>: finds the block", () => {
    page(`<div class="caixa"><div><input id="u" placeholder="Usuário"></div><div><input type="password" id="p"></div><div role="button">Entrar</div></div>`);
    const ctx = contextFor($("#p"), visible)!;
    expect(ctx.username).toBe($("#u"));
  });

  it("ignora campos escondidos, de busca e desabilitados / ignores hidden, search and disabled fields", () => {
    page(`<form><input type="search" name="q"><input name="login" hidden><input name="conta" id="c"><input type="password" style="display:none"><input type="password" id="p"><input type="password" disabled></form>`);
    const ctx = contextFor($("#p"), visible)!;
    expect(ctx.passwords).toHaveLength(1);
    expect(ctx.username).toBe($("#c"));
    expect(ctx.kind).toBe("login");
    expect(contextFor($('input[name="q"]'), visible)).toBeNull();
  });

  it("login em duas etapas: só usuário / two-step login: username only", () => {
    page(`<form><input id="u" autocomplete="username"><button>Avançar</button></form>`);
    expect(contextFor($("#u"), visible)).toMatchObject({ kind: "login", passwords: [] });
    page(`<form><input id="u" name="nome"><button>Avançar</button></form>`);
    expect(contextFor($("#u"), visible)).toBeNull();
  });
});

describe("preenchimento / filling", () => {
  it("preenche usuário e senha com eventos / fills username and password with events", () => {
    page(`<form><input name="email"><input type="password" id="pw"></form>`);
    const events: string[] = [];
    $("#pw").addEventListener("input", () => events.push("input"));
    $("#pw").addEventListener("change", () => events.push("change"));
    const ctx = contextFor($("#pw"), visible)!;
    expect(fillLogin(ctx, "ana@x.com", "S3nh@", visible)).toBe(2);
    expect($('input[name="email"]').value).toBe("ana@x.com");
    expect($("#pw").value).toBe("S3nh@");
    expect(events).toEqual(["input", "change"]);
  });

  it("não preenche campo que sumiu / does not fill a field that disappeared", () => {
    page(`<form><input name="email"><input type="password" id="pw"></form>`);
    const ctx = contextFor($("#pw"), visible)!;
    $("#pw").style.display = "none";
    expect(fillLogin(ctx, "ana", "x", visible)).toBe(1);
    expect($("#pw").value).toBe("");
  });

  it("senha nova vai para nova e confirmação, nunca para a atual / new password goes to new and confirm, never to current", () => {
    page(`<form><input type="password" id="cur" autocomplete="current-password"><input type="password" id="n1"><input type="password" id="n2"></form>`);
    const ctx = contextFor($("#n1"), visible)!;
    expect(ctx.kind).toBe("signup");
    expect(fillNewPassword(ctx, "Nova!", visible)).toBe(2);
    expect([$("#cur").value, $("#n1").value, $("#n2").value]).toEqual(["", "Nova!", "Nova!"]);
    // Sem dica: 3 campos → o primeiro é o atual. / No hint: 3 fields → the first is current.
    page(`<form><input type="password" id="a"><input type="password" id="b"><input type="password" id="c"></form>`);
    const ctx2 = contextFor($("#a"), visible)!;
    fillNewPassword(ctx2, "Z", visible);
    expect([$("#a").value, $("#b").value, $("#c").value]).toEqual(["", "Z", "Z"]);
  });

  it("lê o que foi digitado / reads what was typed", () => {
    page(`<form><input name="user"><input type="password" id="a"><input type="password" id="b"></form>`);
    $('input[name="user"]').value = "  bia ";
    $("#a").value = "segredo";
    expect(readCredentials(contextFor($("#a"), visible)!)).toEqual({ username: "bia", password: "segredo" });
  });
});
