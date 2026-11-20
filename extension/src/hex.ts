// Hexadecimal: o formato dos bytes nas mensagens com o app.
// Hexadecimal: the byte format in messages with the app.

export function toHex(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

/** Lê hexadecimal; lança erro se for inválido. / Reads hex; throws if invalid. */
export function fromHex(text: string): Uint8Array {
  if (text.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(text)) throw new Error("hex inválido / invalid hex");
  const out = new Uint8Array(text.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(text.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** Bytes aleatórios do navegador. / Random bytes from the browser. */
export function randomBytes(n: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(n));
}
