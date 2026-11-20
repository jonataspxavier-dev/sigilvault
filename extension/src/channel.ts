// Canal criptografado de uma conexão: o mesmo esquema de `crates/pairing/src/channel.rs`, feito
// com WebCrypto. As chaves são "não extraíveis": nem este código consegue lê-las.
// Encrypted channel of one connection: the same scheme as `crates/pairing/src/channel.rs`, built
// with WebCrypto. Keys are "non-extractable": not even this code can read them.
//
// - Chaves por direção: HKDF-SHA256(sal = nonce_extensão || nonce_app, chave de pareamento).
//   Per-direction keys: HKDF-SHA256(salt = extension_nonce || app_nonce, pairing key).
// - AES-256-GCM, nonce = 4 zeros + contador de 64 bits; fora de ordem = recusado.
//   AES-256-GCM, nonce = 4 zeros + 64-bit counter; out of order = refused.

const enc = new TextEncoder();
const INFO_EXT_TO_APP = enc.encode("SigilVault session v1 extension->app");
const INFO_APP_TO_EXT = enc.encode("SigilVault session v1 app->extension");
const AAD = enc.encode("SigilVault channel v1");

/** Importa a chave de pareamento como não extraível. / Imports the pairing key as non-extractable. */
export function importPairingKey(raw: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", raw as BufferSource, "HKDF", false, ["deriveKey"]);
}

function nonce(counter: number): Uint8Array {
  const n = new Uint8Array(12);
  new DataView(n.buffer).setBigUint64(4, BigInt(counter));
  return n;
}

export class Channel {
  private sendCounter = 0;
  private recvCounter = 0;

  private constructor(
    private readonly sendKey: CryptoKey,
    private readonly recvKey: CryptoKey,
  ) {}

  /** Deriva as chaves da conexão (lado da extensão). / Derives the connection keys (extension side). */
  static async create(pairingKey: CryptoKey, extensionNonce: Uint8Array, appNonce: Uint8Array): Promise<Channel> {
    const salt = new Uint8Array(64);
    salt.set(extensionNonce, 0);
    salt.set(appNonce, 32);
    const derive = (info: Uint8Array, usage: KeyUsage) =>
      crypto.subtle.deriveKey({ name: "HKDF", hash: "SHA-256", salt, info: info as BufferSource }, pairingKey, { name: "AES-GCM", length: 256 }, false, [usage]);
    return new Channel(await derive(INFO_EXT_TO_APP, "encrypt"), await derive(INFO_APP_TO_EXT, "decrypt"));
  }

  /** Criptografa a próxima mensagem. O contador avança antes de esperar, então chamadas em
   *  sequência mantêm a ordem. / Encrypts the next message. The counter advances before awaiting,
   *  so sequential calls keep their order. */
  async seal(plain: Uint8Array): Promise<Uint8Array> {
    const iv = nonce(this.sendCounter++);
    return new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv: iv as BufferSource, additionalData: AAD as BufferSource }, this.sendKey, plain as BufferSource));
  }

  /** Abre a próxima mensagem; lança erro se adulterada ou fora de ordem. Chamar uma de cada vez.
   *  Opens the next message; throws if tampered or out of order. Call one at a time. */
  async open(sealed: Uint8Array): Promise<Uint8Array> {
    const iv = nonce(this.recvCounter);
    const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: iv as BufferSource, additionalData: AAD as BufferSource }, this.recvKey, sealed as BufferSource);
    this.recvCounter++;
    return new Uint8Array(plain);
  }
}
