import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { MalformedCodeError, type Pake, loadPake } from "./pake";

// Usa o WebAssembly compilado do Rust (cargo build -p sigilvault-pairing --target wasm32-unknown-unknown --release).
// Uses the WebAssembly built from Rust.
const WASM = join(__dirname, "..", "..", "target", "wasm32-unknown-unknown", "release", "sigilvault_pairing.wasm");

let pake: Pake;
beforeAll(async () => {
  pake = await loadPake(readFileSync(WASM));
});

describe("pareamento (WebAssembly) / pairing (WebAssembly)", () => {
  it("gera o pedido com código válido / builds the request with a valid code", () => {
    const a = pake.loginStart("abcd-1234");
    const b = pake.loginStart("ABCD 1234");
    // Pedido OPAQUE: elemento cego (32) + nonce (32) + chave efêmera (32). / OPAQUE request: blinded element + nonce + ephemeral key.
    expect(a.length).toBe(96);
    // Sorteado de novo a cada vez. / Freshly random each time.
    expect(a).not.toEqual(b);
  });

  it("recusa código malformado / rejects malformed code", () => {
    for (const bad of ["", "ABCD-123", "ABCD-1234-5", "ABCU-1234", "ÁBCD-1234"]) {
      expect(() => pake.loginStart(bad)).toThrow(MalformedCodeError);
    }
  });

  it("resposta inválida não passa / invalid response does not pass", () => {
    pake.loginStart("ABCD-1234");
    expect(() => pake.loginFinish(new Uint8Array(10))).not.toThrow(MalformedCodeError);
    expect(() => pake.pairingKey(new Uint8Array(16))).toThrow(); // fora de ordem / out of order
  });
});
