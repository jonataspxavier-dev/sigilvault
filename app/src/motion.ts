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

const SEAL_SHAPE = "M45 8l7 5 9-1 3 8 8 4-2 9 5 7-6 6 1 9-9 3-4 8-9-2-7 6-7-6-9 2-4-8-9-3 1-9-6-6 5-7-2-9 8-4 3-8 9 1z";

/** Face do selo (cera, anel e runa ↑). / Seal face (wax, ring and ↑ rune). */
function sealFace(): SVGElement[] {
  return [
    s("path", { d: SEAL_SHAPE, fill: "url(#sv-wax)" }),
    s("circle", { cx: 45, cy: 45, r: 21, fill: "none", stroke: "#e98a7e", "stroke-opacity": 0.5, "stroke-width": 2 }),
    s("path", { d: "M45 33v24M38 40l7-7 7 7", stroke: "#f3c8b8", "stroke-opacity": 0.75, "stroke-width": 3, fill: "none", "stroke-linecap": "round" }),
  ];
}

/** Selo de cera que racha e se abre por cima da tela (ao desbloquear). / Wax seal that cracks open over the screen (on unlock). */
export function sealBreak(): void {
  const defs = s(
    "defs",
    {},
    s(
      "radialGradient",
      { id: "sv-wax", cx: 0.38, cy: 0.32, r: 0.75 },
      s("stop", { offset: 0, "stop-color": "#cf4141" }),
      s("stop", { offset: 0.6, "stop-color": "#9b2226" }),
      s("stop", { offset: 1, "stop-color": "#5e1014" }),
    ),
    s("clipPath", { id: "sv-cl" }, s("path", { d: "M0 0H47L40 30 50 48 41 66 46 90H0Z" })),
    s("clipPath", { id: "sv-cr" }, s("path", { d: "M47 0H90V90H46L41 66 50 48 40 30Z" })),
  );
  const overlay = document.createElement("div");
  overlay.className = "seal-overlay";
  overlay.setAttribute("aria-hidden", "true");
  overlay.append(
    svgRoot(
      "0 0 90 90",
      "seal-break",
      defs,
      s("g", { class: "half left", "clip-path": "url(#sv-cl)" }, ...sealFace()),
      s("g", { class: "half right", "clip-path": "url(#sv-cr)" }, ...sealFace()),
      s("path", { class: "crack", d: "M47 4L40 30 50 48 41 66 46 86", stroke: "#1a0506", "stroke-width": 2, fill: "none" }),
    ),
  );
  document.body.append(overlay);
  window.setTimeout(() => overlay.remove(), 1500);
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
