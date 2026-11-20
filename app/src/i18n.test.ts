// Garante que as duas línguas têm as mesmas chaves e os mesmos {marcadores}.
// Ensures both languages have the same keys and the same {placeholders}.
import { describe, expect, it } from "vitest";
import { dictionaries, format } from "./i18n";

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("i18n", () => {
  it("pt e en têm as mesmas chaves / same keys", () => {
    expect(Object.keys(dictionaries.en).sort()).toEqual(Object.keys(dictionaries.pt).sort());
  });

  it("marcadores iguais nas duas línguas / same placeholders", () => {
    for (const key of Object.keys(dictionaries.pt) as (keyof typeof dictionaries.pt)[]) {
      expect(placeholders(dictionaries.en[key]), key).toEqual(placeholders(dictionaries.pt[key]));
    }
  });

  it("nenhum texto vazio / no empty text", () => {
    for (const dict of Object.values(dictionaries)) for (const v of Object.values(dict)) expect(v.trim()).not.toBe("");
  });

  it("format troca marcadores conhecidos / replaces known placeholders", () => {
    expect(format("Olá, {name}! {x}", { name: "Ana" })).toBe("Olá, Ana! {x}");
  });

  it("todo código de erro do Rust tem tradução / every Rust error code is translated", () => {
    const codes = ["invalid_credentials", "malformed_code", "tampered", "invalid_file", "weak_password", "invalid_generator_options", "not_found", "invalid_input", "no_codes_left", "io", "internal", "locked", "locked_out", "vault_exists", "clipboard", "unsupported_import", "use_merge", "conflict", "file_too_large"];
    for (const c of codes) expect(Object.keys(dictionaries.pt)).toContain(`err_${c}`);
  });
});
