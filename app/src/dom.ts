// Montagem de elementos sem innerHTML: todo texto entra como nó de texto, então nada vindo do
// cofre (títulos, notas) pode virar código na página.
// Element building without innerHTML: every text goes in as a text node, so nothing coming from
// the vault (titles, notes) can turn into code on the page.

export type Child = Node | string | number | null | undefined | false;

type Props = Record<string, unknown>;

const FORBIDDEN = new Set(["innerHTML", "outerHTML", "insertAdjacentHTML", "srcdoc"]);

/** Cria um elemento. `onClick` etc. viram ouvintes; o resto vira propriedade ou atributo.
 *  Creates an element. `onClick` etc. become listeners; the rest become properties or attributes. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Props | null = null,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props ?? {})) {
    if (value === undefined || value === null || value === false) continue;
    if (FORBIDDEN.has(key)) throw new Error(`propriedade proibida / forbidden property: ${key}`);
    if (key === "class") {
      el.className = String(value);
    } else if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2).toLowerCase(), value as EventListener);
    } else if (key.startsWith("aria-") || key.startsWith("data-") || key === "role" || key === "for") {
      el.setAttribute(key, String(value));
    } else if (key in el) {
      (el as unknown as Record<string, unknown>)[key] = value;
    } else {
      el.setAttribute(key, String(value));
    }
  }
  append(el, ...children);
  return el;
}

export function append(parent: Node, ...children: Child[]): void {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    parent.appendChild(typeof c === "string" || typeof c === "number" ? document.createTextNode(String(c)) : c);
  }
}

/** Troca todo o conteúdo de um elemento. / Replaces all content of an element. */
export function mount(parent: Element, ...children: Child[]): void {
  parent.replaceChildren();
  append(parent, ...children);
}
