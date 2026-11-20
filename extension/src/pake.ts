// Lado da extensão do pareamento OPAQUE: carrega o WebAssembly compilado de `crates/pairing`.
// O código digitado e a chave de sessão ficam dentro da memória do WebAssembly e são apagados
// ao terminar.
// Extension side of OPAQUE pairing: loads the WebAssembly built from `crates/pairing`. The typed
// code and the session key stay inside WebAssembly memory and are wiped at the end.

/** Código com formato inválido (antes de falar com o app). / Malformed code (before talking to the app). */
export class MalformedCodeError extends Error {}

export interface Pake {
  /** Passo 1: devolve o pedido para o app. / Step 1: returns the request for the app. */
  loginStart(code: string): Uint8Array;
  /** Passo 2: devolve a mensagem final, ou null se o código estiver errado. / Step 2: final message, or null if the code is wrong. */
  loginFinish(response: Uint8Array): Uint8Array | null;
  /** Passo 3: chave de pareamento (32 bytes). Apague-a depois de importar. / Step 3: pairing key (32 bytes). Wipe after importing. */
  pairingKey(id: Uint8Array): Uint8Array;
  /** Apaga o estado. / Wipes the state. */
  reset(): void;
}

interface Exports {
  memory: WebAssembly.Memory;
  sv_alloc(len: number): number;
  sv_free(ptr: number, len: number): void;
  sv_out_ptr(): number;
  sv_out_len(): number;
  sv_reset(): void;
  sv_login_start(ptr: number, len: number): number;
  sv_login_finish(ptr: number, len: number): number;
  sv_pairing_key(ptr: number, len: number): number;
}

const OK = 0;
const REJECTED = 1;
const MALFORMED = 2;

/** Carrega o módulo a partir dos bytes do arquivo `.wasm`. / Loads the module from the `.wasm` bytes. */
export async function loadPake(wasm: BufferSource): Promise<Pake> {
  let memory: WebAssembly.Memory | undefined;
  const imports = {
    sigilvault: {
      // Acaso do navegador escrito direto na memória do WebAssembly. / Browser randomness written into WebAssembly memory.
      random_fill(ptr: number, len: number): void {
        crypto.getRandomValues(new Uint8Array(memory!.buffer, ptr, len));
      },
    },
  };
  const { instance } = await WebAssembly.instantiate(wasm, imports);
  const x = instance.exports as unknown as Exports;
  memory = x.memory;

  // Chama uma função com uma entrada; a entrada é apagada da memória logo depois.
  // Calls a function with one input; the input is wiped from memory right after.
  const call = (fn: (ptr: number, len: number) => number, input: Uint8Array): number => {
    const ptr = x.sv_alloc(input.length);
    new Uint8Array(x.memory.buffer, ptr, input.length).set(input);
    try {
      return fn(ptr, input.length);
    } finally {
      x.sv_free(ptr, input.length);
    }
  };
  const output = (): Uint8Array => new Uint8Array(x.memory.buffer, x.sv_out_ptr(), x.sv_out_len()).slice();

  return {
    loginStart(code) {
      const bytes = new TextEncoder().encode(code);
      const rc = call(x.sv_login_start, bytes);
      bytes.fill(0);
      if (rc === MALFORMED) throw new MalformedCodeError();
      if (rc !== OK) throw new Error(`pake ${rc}`);
      return output();
    },
    loginFinish(response) {
      const rc = call(x.sv_login_finish, response);
      if (rc === REJECTED) return null;
      if (rc !== OK) throw new Error(`pake ${rc}`);
      return output();
    },
    pairingKey(id) {
      const rc = call(x.sv_pairing_key, id);
      if (rc !== OK) throw new Error(`pake ${rc}`);
      const key = output();
      x.sv_reset();
      return key;
    },
    reset() {
      x.sv_reset();
    },
  };
}
