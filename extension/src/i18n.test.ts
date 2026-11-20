import { describe, expect, it } from "vitest";

import { formatCode } from "./format";
import { fromHex, toHex } from "./hex";
import { detectLang, dictionaries, errorText, setLang, t } from "./i18n";

describe("textos / strings", () => {
  it("as duas línguas têm as mesmas chaves, sem texto vazio / both languages have the same keys, no empty text", () => {
    expect(Object.keys(dictionaries.en).sort()).toEqual(Object.keys(dictionaries.pt).sort());
    for (const d of Object.values(dictionaries)) for (const v of Object.values(d)) expect(v.trim()).not.toBe("");
  });

  it("escolhe o idioma pelo navegador / picks the language from the browser", () => {
    expect(detectLang("pt-BR")).toBe("pt");
    expect(detectLang("pt")).toBe("pt");
    expect(detectLang("en-US")).toBe("en");
    expect(detectLang(undefined)).toBe("en");
  });

  it("traduz erros conhecidos e cai no genérico / translates known errors and falls back", () => {
    setLang("pt");
    expect(errorText("bad_code")).toBe(t("err_bad_code"));
    expect(errorText("algo_estranho")).toBe(t("err_generic"));
    expect(t("status_unlocked_user", { name: "Ana" })).toBe("Cofre de Ana aberto");
  });
});

describe("formatação / formatting", () => {
  it("formata o código / formats the code", () => {
    expect(formatCode("abcd1234")).toBe("ABCD-1234");
    expect(formatCode("ab")).toBe("AB");
    expect(formatCode("ab cd-12 34 99")).toBe("ABCD-1234");
    expect(formatCode("<script>")).toBe("SCRI-PT");
  });

  it("hexadecimal ida e volta, e recusa lixo / hex roundtrip, and rejects junk", () => {
    expect(toHex(fromHex("00ff10"))).toBe("00ff10");
    expect(() => fromHex("abc")).toThrow();
    expect(() => fromHex("zz")).toThrow();
  });
});
