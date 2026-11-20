import { describe, expect, it } from "vitest";

import { Channel, importPairingKey } from "./channel";
import { fromHex, toHex } from "./hex";

const enc = new TextEncoder();
const seq = (start: number) => Uint8Array.from({ length: 32 }, (_, i) => start + i);

/** O lado do app, imitado com WebCrypto (as direções trocadas). / The app side, imitated with WebCrypto (directions swapped). */
async function appSide(pairing: CryptoKey, e: Uint8Array, a: Uint8Array) {
  const salt = new Uint8Array([...e, ...a]);
  const derive = (info: string, usage: KeyUsage) =>
    crypto.subtle.deriveKey({ name: "HKDF", hash: "SHA-256", salt, info: enc.encode(info) }, pairing, { name: "AES-GCM", length: 256 }, false, [usage]);
  return {
    send: await derive("SigilVault session v1 app->extension", "encrypt"),
    recv: await derive("SigilVault session v1 extension->app", "decrypt"),
  };
}

function iv(counter: number) {
  const n = new Uint8Array(12);
  new DataView(n.buffer).setBigUint64(4, BigInt(counter));
  return n;
}

const AAD = enc.encode("SigilVault channel v1");

describe("canal / channel", () => {
  // Mesmo vetor de `crates/pairing/src/channel.rs`: prova que Rust e WebCrypto batem.
  // Same vector as `crates/pairing/src/channel.rs`: proves Rust and WebCrypto agree.
  it("bate com o vetor fixo do Rust / matches the Rust fixed vector", async () => {
    const key = await importPairingKey(Uint8Array.from({ length: 32 }, (_, i) => i));
    const ch = await Channel.create(key, seq(0x40), seq(0x80));
    expect(toHex(await ch.seal(enc.encode('{"op":"status"}')))).toBe("329498716e49bc206520b1628feba923d6439fc4bf6e8346c3409b74df0321");
    const fromApp = fromHex("120bcafbe19f2bc94cb64873e4e066e602f09b93a3e758073dc1e7cd00186f");
    expect(new TextDecoder().decode(await ch.open(fromApp))).toBe('{"locked":true}');
  });

  it("recusa repetida, fora de ordem e adulterada / refuses replayed, reordered and tampered", async () => {
    const key = await importPairingKey(new Uint8Array(32).fill(1));
    const e = seq(2);
    const a = seq(3);
    const ext = await Channel.create(key, e, a);
    const app = await appSide(key, e, a);
    const seal = async (n: number, text: string) =>
      new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv: iv(n), additionalData: AAD }, app.send, enc.encode(text)));
    const m0 = await seal(0, "zero");
    const m1 = await seal(1, "um");
    await expect(ext.open(m1)).rejects.toThrow();
    expect(new TextDecoder().decode(await ext.open(m0))).toBe("zero");
    await expect(ext.open(m0)).rejects.toThrow();
    const bad = m1.slice();
    bad[0]! ^= 1;
    await expect(ext.open(bad)).rejects.toThrow();
    expect(new TextDecoder().decode(await ext.open(m1))).toBe("um");

    // O que a extensão envia o app abre, em ordem. / What the extension sends the app opens, in order.
    const c0 = await ext.seal(enc.encode("a"));
    const c1 = await ext.seal(enc.encode("b"));
    const open = async (n: number, c: Uint8Array) =>
      new TextDecoder().decode(await crypto.subtle.decrypt({ name: "AES-GCM", iv: iv(n), additionalData: AAD }, app.recv, c as BufferSource));
    expect(await open(0, c0)).toBe("a");
    expect(await open(1, c1)).toBe("b");
  });

  it("a chave de pareamento não pode ser lida / the pairing key cannot be read", async () => {
    const key = await importPairingKey(new Uint8Array(32).fill(9));
    expect(key.extractable).toBe(false);
    await expect(crypto.subtle.exportKey("raw", key)).rejects.toThrow();
  });
});
