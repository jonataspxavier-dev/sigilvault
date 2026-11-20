// Formatação de entrada do usuário. / User input formatting.

/** Formata o código de pareamento enquanto o usuário digita: ABCD-1234.
 *  Formats the pairing code while the user types: ABCD-1234. */
export function formatCode(raw: string): string {
  const clean = raw.toUpperCase().replace(/[^0-9A-Z]/g, "").slice(0, 8);
  return clean.length > 4 ? `${clean.slice(0, 4)}-${clean.slice(4)}` : clean;
}
