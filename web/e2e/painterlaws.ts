// The painter's laws, run in a page by `painter.test.ts`: a page's program driven through `painter.ts` and `run.ts` with
// a pure handler, and after every step the painted DOM compared, node by node, with what a fresh painter draws of the
// program's current view, and with a second run of the same steps.

import painter from "../src/painter/painter";
import run from "../src/painter/run";

export namespace painterlaws {
  // A program's class as `viewtree::export!` makes it, with the fresh draw the laws compare against.
  export type Program = run.Module & { readonly fresh: () => Uint8Array };

  // What one step does: hand the program an input of the runtime's, tap the `at`-th button, type into the `at`-th field,
  // or answer the `at`-th act still waiting.
  export type Step =
    | { readonly type: "input"; readonly input: unknown }
    | { readonly type: "click"; readonly at: number }
    | { readonly type: "type"; readonly at: number; readonly words: string }
    | { readonly type: "answer"; readonly at: number; readonly body: unknown; readonly refused: string | null };

  // The first law that failed, at which step and where in the tree, or none.
  export type Report =
    | { readonly type: "Held"; readonly steps: number; readonly nodes: number; readonly effects: number }
    | { readonly type: "Broke"; readonly law: string; readonly step: number; readonly at: string; readonly painted: string; readonly expected: string };

  // A node as a reader of the page sees it: its tag, attributes and the field's value, or its text; never an id.
  type Seen = { readonly tag: string; readonly attrs: readonly (readonly [string, string])[]; readonly value: string | null; readonly kids: readonly Seen[] } | string;

  const seen = (n: Node): Seen =>
    n instanceof Element
      ? {
          tag: n.tagName.toLowerCase(),
          attrs: [...n.attributes].map(a => [a.name, a.value] as const).sort(([a], [b]) => (a < b ? -1 : 1)),
          value: n instanceof HTMLInputElement ? n.value : null,
          kids: [...n.childNodes].map(seen),
        }
      : (n.textContent ?? "");

  const print = (s: Seen | undefined): string => (s === undefined ? "nothing" : JSON.stringify(s).slice(0, 400));

  // Where two trees first differ, as a path of child indices, or null when they are the same.
  const differ = (a: Seen | undefined, b: Seen | undefined, at: string): readonly [string, string, string] | null => {
    if (typeof a === "string" || typeof b === "string" || a === undefined || b === undefined)
      return a === b ? null : [at, print(a), print(b)];
    if (a.tag !== b.tag || a.value !== b.value || JSON.stringify(a.attrs) !== JSON.stringify(b.attrs) || a.kids.length !== b.kids.length)
      return [at, print({ ...a, kids: [] }), print({ ...b, kids: [] })];
    for (let i = 0; i < a.kids.length; i++) {
      const d = differ(a.kids[i], b.kids[i], `${at}/${i}`);
      if (d !== null) return d;
    }
    return null;
  };

  // A foreign part drawn from its kind, key and props alone, so two painters draw the same one.
  const foreign: painter.Foreign = (kind, key, props) => {
    const n = document.createElement("div");
    n.setAttribute("data-foreign", kind);
    n.setAttribute("data-key", key);
    n.textContent = props;
    return n;
  };

  // An effect answered once: a GET, a POST or a new key.
  type Ask = { readonly type: string; readonly ask: unknown };
  const isAsk = (e: unknown): e is Ask =>
    typeof e === "object" && e !== null && "type" in e && (e.type === "Get" || e.type === "Post" || e.type === "NewKey") && "ask" in e;

  // One run of the steps: its root, every effect it asked for in order, a step, and the patch bytes of a reading handed
  // again.
  type Ran = {
    readonly root: HTMLElement;
    readonly effects: readonly unknown[];
    readonly program: Program;
    readonly step: (s: Step) => void;
    readonly again: (input: unknown) => number;
    readonly destroy: () => void;
  };

  // The device holds no block: a block shows its plain text, in both draws alike.
  const held: painter.Blocks = () => Promise.resolve(null);

  const started = (make: () => Program): Ran => {
    const root = document.createElement("div");
    const program = make();
    const effects: unknown[] = [];
    const waiting: Ask[] = [];
    const page = run.start(root, program, foreign, held, e => {
      effects.push(e);
      if (isAsk(e)) waiting.push(e);
    }, undefined, () => document.scrollingElement ?? document.documentElement);
    const step = (s: Step): void => {
      switch (s.type) {
        case "input":
          return page.send(s.input);
        case "click": {
          const buttons = root.querySelectorAll("button");
          return buttons.length === 0 ? undefined : buttons[s.at % buttons.length]?.click();
        }
        case "type": {
          const fields = root.querySelectorAll("input");
          const f = fields.length === 0 ? undefined : fields[s.at % fields.length];
          if (f === undefined) return;
          f.value = s.words;
          return void f.dispatchEvent(new Event("input", { bubbles: true }));
        }
        case "answer": {
          const [ask] = waiting.length === 0 ? [] : waiting.splice(s.at % waiting.length, 1);
          if (ask === undefined) return;
          return page.send({ type: "Answered", ask: ask.ask, answer: s.refused === null ? { type: "Done", body: s.body } : { type: "Refused", status: 403, words: s.refused } });
        }
      }
    };
    // The bytes an input handed a second time patches: none, if painting a view again patches nothing.
    const again = (input: unknown): number => program.input(JSON.stringify(input)).length;
    return { root, effects, program, step, again, destroy: page.destroy };
  };

  const drawnFresh = (p: Program): HTMLElement => {
    const root = document.createElement("div");
    painter.create(root, foreign, held, () => false).apply(p.fresh());
    return root;
  };

  const count = (n: Node): number => 1 + [...n.childNodes].reduce((k, c) => k + count(c), 0);

  /**
   * The laws over one run of `steps`:
   * 1. a patch applied equals a fresh draw of the new view, after every step;
   * 2. the view is a function of the model: a second run of the same steps paints the same DOM and asks the same effects;
   * 3. painting a view again patches nothing: an input handed twice patches nothing the second time.
   */
  export const check = (make: () => Program, steps: readonly Step[]): Report => {
    const a = started(make);
    const b = started(make);
    let nodes = 0;
    for (let i = 0; i < steps.length; i++) {
      const s = steps[i];
      if (s === undefined) continue;
      a.step(s);
      b.step(s);
      const fresh = differ(seen(a.root), seen(drawnFresh(a.program)), "");
      if (fresh !== null) return { type: "Broke", law: "a patch applied equals a fresh draw", step: i, at: fresh[0], painted: fresh[1], expected: fresh[2] };
      const replayed = differ(seen(a.root), seen(b.root), "");
      if (replayed !== null) return { type: "Broke", law: "the same steps paint the same DOM", step: i, at: replayed[0], painted: replayed[1], expected: replayed[2] };
      if (JSON.stringify(a.effects) !== JSON.stringify(b.effects))
        return { type: "Broke", law: "the same steps ask the same effects", step: i, at: "", painted: JSON.stringify(a.effects), expected: JSON.stringify(b.effects) };
      if (s.type === "input") {
        const [x, y] = [a.again(s.input), b.again(s.input)];
        if (x !== 0 || y !== 0) return { type: "Broke", law: "painting a view again patches nothing", step: i, at: "", painted: `${x} and ${y} bytes`, expected: "0 bytes" };
      }
      nodes = Math.max(nodes, count(a.root));
    }
    const effects = a.effects.length;
    for (const r of [a, b]) r.destroy();
    return { type: "Held", steps: steps.length, nodes, effects };
  };
}
export default painterlaws;
