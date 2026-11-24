// Animações medievais curtas: tocam uma vez e somem. Só mexem em transform, opacity e clip-path
// (a placa de vídeo cuida, sem recalcular a página). Ficam sempre ligadas (escolha do projeto).
// Os desenhos são montados elemento por elemento, sem innerHTML (regra do projeto, ver dom.ts).
// Short medieval animations: they play once and go away. Only transform, opacity and clip-path
// (handled by the GPU, no page relayout). Always on (project choice). Drawings are built element
// by element, without innerHTML (project rule, see dom.ts).

const NS = "http://www.w3.org/2000/svg";

type Attrs = Record<string, string | number>;

/** Cria um elemento SVG com atributos e filhos. / Creates an SVG element with attributes and children. */
function s(tag: string, attrs: Attrs = {}, ...children: SVGElement[]): SVGElement {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  el.append(...children);
  return el;
}

function svgRoot(viewBox: string, cls: string, ...children: SVGElement[]): SVGSVGElement {
  return s("svg", { viewBox, class: cls, "aria-hidden": "true" }, ...children) as SVGSVGElement;
}

/** "SIGIL" em runas (Elder Futhark) que se desenham. / "SIGIL" in runes (Elder Futhark) that draw themselves. */
export function runes(): SVGSVGElement {
  const strokes = ["M24 8L12 24l14 12-14 16", "M50 8v44", "M72 10l32 40M104 10L72 50", "M126 8v44", "M150 52V8l14 12"];
  return svgRoot("0 0 170 60", "runes", ...strokes.map((d) => s("path", { d })));
}

/** Fumaça subindo ao fundo (só na tela de desbloqueio). / Smoke rising in the background (unlock screen only). */
export function smoke(): HTMLElement {
  const el = document.createElement("div");
  el.className = "smoke";
  el.setAttribute("aria-hidden", "true");
  el.append(document.createElement("i"), document.createElement("i"), document.createElement("i"));
  return el;
}

/** O dragão desperta (ao desbloquear): sai da penumbra, os olhos acendem em brasa, um anel de runas
 *  se expande e tudo some revelando o cofre, que já carrega por baixo.
 *  The dragon awakens (on unlock): it comes out of the shadows, its eyes light up like embers, a
 *  ring of runes expands and everything fades revealing the vault, already loading underneath. */
export function dragonWake(): void {
  const overlay = document.createElement("div");
  overlay.className = "wake-overlay";
  overlay.setAttribute("aria-hidden", "true");

  const ticks = "M100 4v14M100 182v14M4 100h14M182 100h14M32 32l10 10M158 158l10 10M32 168l10-10M158 42l10-10";
  const ring = svgRoot("0 0 200 200", "wake-ring", s("circle", { cx: 100, cy: 100, r: 90 }), s("path", { d: ticks }));

  const dragon = document.createElement("div");
  dragon.className = "wake-dragon";
  const img = document.createElement("img");
  img.src = "/dragao.png";
  img.alt = "";
  const eyes = document.createElement("div");
  eyes.className = "wake-eyes";
  dragon.append(img, eyes);

  overlay.append(ring, dragon);
  document.body.append(overlay);
  window.setTimeout(() => overlay.remove(), 2000);
}
/** Faíscas douradas saindo de um elemento (ao copiar). / Golden sparks bursting from an element (on copy). */
export function sparks(from: Element): void {
  const r = from.getBoundingClientRect();
  const layer = document.createElement("div");
  layer.className = "spark-layer";
  layer.style.left = `${r.left + r.width / 2}px`;
  layer.style.top = `${r.top + r.height / 2}px`;
  for (let i = 0; i < 12; i++) {
    const spark = document.createElement("span");
    spark.className = "spark";
    const a = (i / 12) * Math.PI * 2;
    const d = 28 + Math.random() * 16;
    spark.style.setProperty("--dx", `${Math.cos(a) * d}px`);
    spark.style.setProperty("--dy", `${Math.sin(a) * d}px`);
    layer.append(spark);
  }
  document.body.append(layer);
  window.setTimeout(() => layer.remove(), 700);
}

/** Portão de ferro descendo sobre a janela; resolve quando fecha (ao bloquear).
 *  Iron gate dropping over the window; resolves once closed (on lock). */
export function gateClose(): Promise<void> {
  const gate = document.createElement("div");
  gate.className = "gate";
  gate.setAttribute("aria-hidden", "true");
  document.body.append(gate);
  return new Promise((resolve) => window.setTimeout(resolve, 650));
}

/** Desenrola um painel de cima para baixo (ao abrir um item). / Unrolls a panel top to bottom (on opening an item). */
export function unroll(el: Element): void {
  el.classList.remove("unroll");
  void (el as HTMLElement).offsetWidth;
  el.classList.add("unroll");
}
