// The chain, verified in this tab: the core compiled to wasm rehashes, parses and orders every object, and the page
// names which checker ran (the SubtleCrypto fallback checks strictly less); the server's own verify is shown beside it.

import { chain } from "../wire/api";
import type { ChainPayload, Link, Proof } from "../wire/generated/wire";
import type { CoreVerdict, Provenance } from "../prodrome/core";
import { loadCore, provenanceOf, verifyObjects } from "../prodrome/core";
import { el } from "../view/dom";
import { chip, iso, shortHash } from "../view/format";
import type { Mounted } from "../view/page";
import { hrefOf } from "../shell/route";
import type { ChainVerdict, ObjectVerdict } from "../prodrome/verify";
import { condition, eventOf, found, summarise, verifyChain } from "../prodrome/verify";
import frame from "../view/frame";
import placeholder from "../view/placeholder";
import rows from "../view/rows";
import asked from "../app/asked";

const hasSubtle = typeof globalThis.crypto !== "undefined" && "subtle" in globalThis.crypto;

/** Who produced the verdict, and what that buys the reader: the two check different amounts. */
interface Checker {
  name: string;
  claim: string;
}

const CORE: Checker = {
  name: "the core itself, compiled to WebAssembly",
  claim:
    "Every object was rehashed AND parsed as an envelope by the core as WebAssembly — the same Rust the server folds with — and the causal order below is the one linearise() put them in.",
};

const FALLBACK: Checker = {
  name: "this page's own SubtleCrypto fallback",
  claim:
    "The WebAssembly core did not load, so the hashes and the parent walk were checked by the hand-written path in web/src/prodrome/verify.ts. It rehashes and it walks; it does NOT parse the stored grammar, so it checks strictly less than the core would have.",
};

interface Checked {
  /** `null` when nothing checked, which the page says rather than showing an empty table as a clean one. */
  verdict: ChainVerdict | CoreVerdict | null;
  /** Who wrote each object, by its signatures, in the core's reading; empty where the core did not run. */
  wrote: ReadonlyMap<string, Provenance>;
  by: Checker;
  /** Why the core did not run, "" when it did. */
  why: string;
}

async function checked(links: readonly Link[], tips: readonly string[], root: string | null): Promise<Checked> {
  try {
    const core = await loadCore();
    const wrote = new Map(provenanceOf(core, links, root).map((p): [string, Provenance] => [p.hash, p]));
    return { verdict: verifyObjects(core, links), wrote, by: CORE, why: "" };
  } catch (error: unknown) {
    const why = `The WebAssembly core did not load, so the fallback ran: ${String(error)}`;
    if (!hasSubtle) return { verdict: null, wrote: new Map(), by: FALLBACK, why };
    return { verdict: await verifyChain(links, tips), wrote: new Map(), by: FALLBACK, why };
  }
}

function linearisationOf(verdict: ChainVerdict | CoreVerdict): string[] {
  return "linearisation" in verdict ? verdict.linearisation : [];
}

// What a proof says in a cell: the key's start, or what stands in for one.
function proofText(proof: Proof): string {
  switch (proof.kind) {
    case "proven":
      return `signed ${proof.key.slice(0, 8)}…`;
    case "predates":
      return "before signatures";
    case "unproven":
      return "not proven";
  }
}

// Who wrote it, by its signatures: a proof anyone holding the objects can check, unlike the actor field.
function wroteCell(p: Provenance | undefined): HTMLElement {
  return el(
    "td",
    { class: `wrote wrote-${p?.proof.kind ?? "none"}`, title: p?.said ?? "" },
    p === undefined ? "" : proofText(p.proof),
  );
}

// How far from the nearest head the walk reached an object, or a dash where it never did.
const depthOf = (v: ObjectVerdict): string => (v.walk.type === "Reached" ? String(v.walk.depth) : "—");

// `index` is the row's place in the whole table, its header the first.
function verdictRow(verdict: ObjectVerdict, wrote: Provenance | undefined, focused: boolean, index: number): HTMLElement {
  const ok = verdict.condition.type === "Clean";
  const event = eventOf(verdict);
  return el(
    "tr",
    {
      class: `${ok ? "ok-row" : "bad-row"}${focused ? " focused" : ""}`,
      "data-todo": event?.todo,
      "data-hash": verdict.hash,
      "aria-rowindex": String(index),
    },
    el("td", null, chip(ok ? "verified" : "FAILED", ok ? "ok" : "prov")),
    el("td", { class: "num" }, depthOf(verdict)),
    el("td", null, shortHash(verdict.hash)),
    el("td", null, verdict.kind),
    el("td", null, event === null ? null : el("a", { href: hrefOf({ type: "todo", id: event.todo }) }, event.todo)),
    el("td", null, event?.actor),
    wroteCell(wrote),
    el("td", { class: "num" }, event === null ? null : iso(event.at)),
    el("td", { class: "problem" }, condition.said(verdict.condition)),
  );
}

function serverOpinion(problems: readonly string[]): HTMLElement {
  return el(
    "section",
    null,
    el("h2", null, "The server's opinion"),
    el(
      "p",
      { class: "legend" },
      "This is what the server's own store.verify() reported. It is not evidence about the server — it is the server's claim about itself, printed here for comparison with the verdict above, which your browser computed.",
    ),
    problems.length === 0
      ? el("p", null, chip("healthy", "ok"), " — store.verify() reported no problems.")
      : el(
          "ul",
          null,
          problems.map((problem) => el("li", { class: "problem" }, problem)),
        ),
  );
}

function heads(tips: readonly string[]): string {
  const first = (tips[0] ?? "").slice(0, 12);
  return tips.length === 1 ? `tip ${first}…` : `${tips.length} heads, starting at ${first}…`;
}

function browserOpinion(outcome: Checked, verdict: ChainVerdict | CoreVerdict, count: number): HTMLElement {
  const { by, why } = outcome;
  const order = linearisationOf(verdict);
  return el(
    "section",
    null,
    el("h2", null, "This browser's verdict"),
    el("p", { class: verdict.found.type === "Reached" ? "verdict ok" : "verdict bad" }, summarise(verdict)),
    // Who checked, before what they found.
    el("p", { class: "checker" }, "Checked by ", el("strong", null, by.name), ". ", by.claim),
    why === "" ? null : el("p", { class: "problem" }, why),
    el(
      "p",
      { class: "legend" },
      `Each object's hash is sha256 of the exact literal beside it — the same bytes events/objects/<hash>.py holds. The walk followed every parent from ${heads(verdict.tips)} through ${verdict.walked} of ${count} objects` +
        (verdict.genesis.length === 0
          ? " and did not reach a genesis."
          : ` to ${verdict.genesis.length === 1 ? "genesis" : `${verdict.genesis.length} genesis objects, starting at`} ${(verdict.genesis[0] ?? "").slice(0, 12)}….`) +
        (order.length === 0 ? "" : ` All ${order.length} of them went into one causal order.`),
    ),
    found.fold(verdict.found, {
      reached: () => null,
      unreached: (problems) => el("ul", null, problems.map((problem) => el("li", { class: "problem" }, problem))),
    }),
  );
}

interface Loaded {
  readonly payload: ChainPayload;
  readonly outcome: Checked;
}

async function loaded(): Promise<Loaded> {
  const payload = await chain();
  return { payload, outcome: await checked(payload.links, payload.heads, payload.root) };
}

// What the filter reads of a row: every cell's words and the whole hash.
function rowText(v: ObjectVerdict, wrote: Provenance | undefined): string {
  const event = eventOf(v);
  return [v.hash, v.walk.type === "Reached" ? v.walk.depth : "", v.kind, event?.todo ?? "", event?.actor ?? "", wroteCell(wrote).textContent, event === null ? "" : iso(event.at), condition.said(v.condition), v.condition.type === "Clean" ? "verified" : "failed"]
    .join(" ")
    .toLowerCase();
}

// Rows either side of those in sight, so a fling shows drawn rows before the next frame draws more.
const MARGIN = 30;

// The objects' table, drawn a window at a time (`view/rows.ts`); every row is one line tall (`#chain td` does not wrap).
// Where the box stands outlives a table drawn again: at the focused object until the reader scrolls.
type Place = { readonly type: "Focus" } | { readonly type: "Scrolled"; readonly top: number };

function objectTable(
  verdict: ChainVerdict | CoreVerdict,
  wrote: ReadonlyMap<string, Provenance>,
  focus: string,
  place: { readonly read: () => Place; readonly keep: (p: Place) => void },
): HTMLElement[] {
  const all = verdict.objects.map((v, i) => ({ v, text: rowText(v, wrote.get(v.hash)), index: i + 2 }));
  const row = (r: (typeof all)[number]): HTMLElement => verdictRow(r.v, wrote.get(r.v.hash), r.v.hash === focus, r.index);
  type Row = (typeof all)[number];
  // Unmeasured until a row has stood in the document; then the rows drawn from, and the window of them drawn.
  type Drawn = { readonly type: "Unmeasured" } | { readonly type: "Measured"; readonly px: number; readonly of: readonly Row[]; readonly w: rows.Window };
  let shown: readonly Row[] = all;
  let drawn: Drawn = { type: "Unmeasured" };
  const body = el("tbody");
  const table = el(
    "table",
    { id: "chain", "aria-rowcount": String(all.length + 1) },
    el(
      "thead",
      null,
      el(
        "tr",
        { "aria-rowindex": "1" },
        ["checked here", "depth", "hash", "kind", "todo", "actor", "signed", "at", "problem"].map((name) => el("th", null, name)),
      ),
    ),
    body,
  );
  const box = el("div", { class: "scroller" }, table);
  const spacer = (): HTMLElement => {
    const cell = el("td", { colspan: "9" });
    return el("tr", { class: "spacer", "aria-hidden": "true" }, cell);
  };
  const above = spacer();
  const below = spacer();
  const hold = (tr: HTMLElement, px: number): void => {
    const cell = tr.firstElementChild;
    if (cell instanceof HTMLElement) cell.style.height = `${px}px`;
  };
  // The rows in the document, by their place in the whole table: a row still in the window stays the same element, so
  // the keyboard's focus and a selection in it survive the scroll that Tab itself makes.
  const kept = new Map<number, HTMLElement>();
  // One row drawn first to measure the rest by.
  const measured = (): number => {
    if (drawn.type === "Measured") return drawn.px;
    const first = all[0];
    if (first === undefined) return 0;
    const one = row(first);
    body.replaceChildren(one);
    const px = one.getBoundingClientRect().height;
    body.replaceChildren(above, below);
    return px;
  };
  const draw = (): void => {
    const px = measured();
    if (px === 0) return;
    const w = rows.of(shown.length, px, box.scrollTop, box.clientHeight, MARGIN);
    if (drawn.type === "Measured" && drawn.of === shown && rows.same(drawn.w, w)) return;
    drawn = { type: "Measured", px, of: shown, w };
    const wanted = shown.slice(w.from, w.to);
    const { gone, added } = rows.moves([...kept.keys()], wanted, (r) => r.index);
    for (const i of gone) {
      kept.get(i)?.remove();
      kept.delete(i);
    }
    for (const { row: r, before } of added) {
      const tr = row(r);
      body.insertBefore(tr, (before === null ? undefined : kept.get(before)) ?? below);
      kept.set(r.index, tr);
    }
    hold(above, w.before);
    hold(below, w.after);
  };
  box.addEventListener(
    "scroll",
    () => {
      place.keep({ type: "Scrolled", top: box.scrollTop });
      draw();
    },
    { passive: true },
  );
  // A box the page took out of the document (the table drawn again for a moved chain) is told once more, and let go.
  const resized = new ResizeObserver(() => (box.isConnected ? draw() : resized.disconnect()));
  resized.observe(box);
  const input = el("input", { type: "search", placeholder: "hash, kind, todo, actor…" });
  const filtered = (): readonly Row[] => {
    const query = input.value.toLowerCase();
    return query === "" ? all : all.filter((r) => r.text.includes(query));
  };
  input.addEventListener("input", () => {
    shown = filtered();
    box.scrollTop = 0;
    draw();
  });
  // Once the box is on the page, where it has a height. A table drawn again keeps the last query in its field
  // (`view/typed.ts`), so the rows are read from the field.
  queueMicrotask(() => {
    shown = filtered();
    draw();
    const was = place.read();
    if (was.type === "Scrolled") box.scrollTop = was.top;
    else {
      const at = shown.findIndex((r) => r.v.hash === focus);
      if (at >= 0 && drawn.type === "Measured") box.scrollTop = rows.centred(at, drawn.px, box.clientHeight);
    }
    draw();
  });
  return [el("label", { class: "filterbox" }, "filter ", input), box];
}

// The same chain checked is the same check: a chain is named by its heads.
const checkedChain = asked.question(loaded, (x, y) => x.payload.heads.join(" ") === y.payload.heads.join(" ") && x.payload.links.length === y.payload.links.length);

export function mount(root: HTMLElement, focus: string): Mounted {
  let place: Place = { type: "Focus" };
  const held = { read: () => place, keep: (p: Place) => void (place = p) };
  return frame.open<Loaded>(root, {
    className: "chainpage",
    wide: true,
    title: "The chain",
    reads: () => checkedChain,
    shape: placeholder.ROWS,
    view: ({ payload, outcome }) => {
      const { verdict } = outcome;
      // Fatal only when both the core and `crypto.subtle` (absent on an insecure origin) are unavailable.
      if (verdict === null) {
        return el(
          "p",
          { class: "legend" },
          `${outcome.why} This browser also exposes no crypto.subtle, so NOTHING on this page has been checked. The second happens on an insecure origin — open the app over https or on localhost.`,
        );
      }
      return [
        browserOpinion(outcome, verdict, payload.links.length),
        serverOpinion(payload.verify),
        el("h2", null, `Objects (${verdict.objects.length}, tip first)`),
        ...objectTable(verdict, outcome.wrote, focus, held),
      ];
    },
  });
}
