// Onde a extensão guarda o pareamento: IndexedDB, com a chave como CryptoKey NÃO extraível.
// O navegador consegue usá-la para derivar as chaves de cada conexão, mas nenhum código (nem o
// nosso) consegue ler os bytes de volta.
// Where the extension keeps the pairing: IndexedDB, with the key as a NON-extractable CryptoKey.
// The browser can use it to derive each connection's keys, but no code (not even ours) can read
// the bytes back.

export interface StoredPairing {
  id: Uint8Array;
  key: CryptoKey;
}

export interface KeyStore {
  load(): Promise<StoredPairing | null>;
  save(pairing: StoredPairing): Promise<void>;
  clear(): Promise<void>;
}

const DB_NAME = "sigilvault";
const STORE = "pairing";
const RECORD = "current";

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function run<T>(mode: IDBTransactionMode, op: (store: IDBObjectStore) => IDBRequest): Promise<T> {
  const db = await openDb();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const req = op(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(req.result as T);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  } finally {
    db.close();
  }
}

/** Armazenamento real (IndexedDB da extensão). / Real storage (the extension's IndexedDB). */
export const indexedDbStore: KeyStore = {
  async load() {
    const value = await run<StoredPairing | undefined>("readonly", (s) => s.get(RECORD));
    return value && value.id instanceof Uint8Array && value.key instanceof CryptoKey ? value : null;
  },
  async save(pairing) {
    await run("readwrite", (s) => s.put(pairing, RECORD));
  },
  async clear() {
    await run("readwrite", (s) => s.delete(RECORD));
  },
};

/** Armazenamento em memória (testes). / In-memory storage (tests). */
export function memoryStore(): KeyStore {
  let value: StoredPairing | null = null;
  return {
    load: async () => value,
    save: async (p) => void (value = p),
    clear: async () => void (value = null),
  };
}
