// One todo as a page: the pushed screen on a phone and the pane beside the list, one view with a back button or without.
// What is read is typst's (the reading part, `reading.ts`); what reacts is here, the long-lived parts as islands.

import { actButton } from "./acts";
import act from "../view/act";
import type { Acted } from "./acts";
import { buildTodo, todo as fetchTodo } from "../wire/api";
import address from "../shell/address";
import type { AuthoredSource, Entry, FieldChange, Link, TodoLinks, TodoPayload } from "../wire/generated/wire";
import { neighbourList } from "./card";
import app from "../app/app";
import { describeConflicts } from "./conflicts";
import type { Child } from "../view/dom";
import { el } from "../view/dom";
import frame from "../view/frame";
import placeholder from "../view/placeholder";
import asked from "../app/asked";
import readings from "../algebra/reading";
import marks from "../view/marks";
import { deferControl, editPanel, subtodoChecklist } from "./editform";
import curve from "./curve";
import { chip, dayOf, hashLink, iso, shortDay } from "../view/format";
import { chevronIcon } from "../view/icons";
import { noteField, noteOf } from "../view/noteline";
import { refusalNotice, waitingNotice } from "../shell/pending";
import { askHeld, resetReplica, watchOthers, watchReplica } from "../prodrome/sync";
import reading from "./reading";
import { askLink } from "../chat/refs";
import { richOrText } from "../rich";
import { hrefOf } from "../shell/route";
import onscreen, { store } from "../app/store";
import { applicable, isOpen, isUnconfirmed } from "./todos";
import thirty from "./thirty";
import { actLabel } from "./undo";
import time from "../app/time";
import price from "./price";
import data from "../algebra/data";
import disclosure from "../view/disclosure";
import ui from "../app/ui";
import registers from "../app/registers";
import dressed from "../chat/dressed";

// Who wrote the content and on which day, in one line; whether its number counts it, behind that line. A todo with no
// content has nothing to attribute, and its content section says so.
function provenance(entry: Entry): HTMLElement | null {
  const authored = entry.authored;
  if (authored === null) return null;
  const unconfirmed = isUnconfirmed(entry);
  const who = authored.actor === "" ? "someone unnamed" : authored.actor;
  const when = authored.at === "" ? "" : ` on ${shortDay(authored.at, app.now().zone, time.now())}`;
  return disclosure.details(
    disclosure.of(ui.cell(registers.told, `${entry.todo}/provenance`)),
    { class: unconfirmed ? "provline unconfirmed" : "provline" },
    [`Written by ${who}${when}`, chevronIcon()],
    el(
      "p",
      null,
      unconfirmed
        ? "Not confirmed yet, so the number above does not use it. Confirm it to make it count."
        : "Confirmed, so the number above uses it.",
    ),
  );
}

function claimNote(entry: Entry): HTMLElement | null {
  if (entry.claimed === "") return null;
  return el(
    "p",
    { class: "claim" },
    chip(`claimed ${entry.claimed}`, "prov"),
    " An untrusted actor recorded this outcome. The pricing fold does not apply it until a trusted actor re-appends it, so the number above still treats this todo as open.",
  );
}

// Concurrent writes to one register, until a later write descends from all of them; nothing when there are none.
// Every candidate is drawn: the number above is the most urgent world's, and no version is the page's pick.
function conflictsBlock(entry: Entry): HTMLElement | null {
  const conflicts = describeConflicts(entry);
  if (conflicts.length === 0) return null;
  return el(
    "section",
    { class: "conflictbox", role: "status" },
    el("h2", null, "Concurrent writes"),
    entry.states.length > 1
      ? el("p", { class: "conflictstate" }, `Its state is ${entry.states.join(" or ")}; it is filed open while any version is.`)
      : null,
    entry.candidates.length > 1
      ? el(
          "ol",
          { class: "candidates" },
          entry.candidates.map((version) =>
            el(
              "li",
              null,
              el("p", { class: "legend" }, `${version.actor}, ${dayOf(version.at, app.now().zone)}`),
              richOrText(version.rich.body, version.body, { class: "todobody" }),
            ),
          ),
        )
      : null,
    el(
      "ul",
      { class: "conflictlist" },
      conflicts.map((conflict) =>
        el(
          "li",
          null,
          el("strong", null, conflict.kind),
          ": ",
          conflict.hashes.map((hash, index) => (index === 0 ? hashLink(hash) : [", ", hashLink(hash)])),
        ),
      ),
    ),
    el(
      "p",
      { class: "conflictwhat" },
      "Two writers changed this at once. Every version stands until one is chosen, and the number is the most urgent version's. Any change you make now settles it.",
    ),
  );
}

function content(entry: Entry, interactive: boolean, acted: Acted): HTMLElement {
  const authored = entry.authored;
  if (authored === null) {
    return el("section", null, el("p", { class: "legend" }, "The chain holds no Authored record for this todo — only lifecycle events."));
  }
  const rich = authored.rich;
  return el(
    "section",
    { class: "todocontent" },
    authored.waiting_on === ""
      ? null
      : [el("h2", null, "Waiting on"), richOrText(rich.waiting_on, authored.waiting_on, { class: "todobody waitingbody" })],
    authored.subtodos.length === 0
      ? null
      : [
          el("h2", null, "Checklist"),
          interactive
            ? subtodoChecklist(entry.todo, authored, acted)
            : el(
                "ul",
                { class: "checklist" },
                authored.subtodos.map((sub) =>
                  el("li", { class: sub.done ? "done" : "" }, el("span", { class: "box" }, sub.done ? "✓" : ""), sub.body),
                ),
              ),
        ],
    // Stored as the lines it was wrapped into; the box rejoins them for typst, so only the fallback shows lines.
    authored.rationale.length === 0
      ? null
      : [
          el("h2", null, "Rationale"),
          rich.rationale === null
            ? el("ul", { class: "rationale" }, authored.rationale.map((line) => el("li", null, line)))
            : richOrText(rich.rationale, authored.rationale.join(" "), { class: "todobody rationalebody" }),
        ],
    authored.source === null ? null : [el("h2", null, "Source message"), sourceBlock(authored.source)],
  );
}

// The message this todo came from, linked by its hash; a record from before hashes has nothing to open.
function sourceBlock(source: AuthoredSource): HTMLElement {
  const facts = el(
    "dl",
    { class: "facts" },
    el("dt", null, "sender"),
    el("dd", null, source.sender),
    el("dt", null, "subject"),
    el("dd", null, source.subject),
    el("dt", null, "date"),
    el("dd", null, source.date),
  );
  if (source.hash === "") {
    return el(
      "div",
      null,
      facts,
      el("p", { class: "legend" }, "No message hash was recorded with this todo, so there is nothing to open."),
    );
  }
  return el(
    "div",
    null,
    el(
      "a",
      { class: "sourcelink", href: hrefOf({ type: "message", id: source.hash }) },
      el("span", { class: "sourcewho" }, source.sender),
      el("span", { class: "sourcesubject" }, source.subject === "" ? "(no subject)" : source.subject),
      el("span", { class: "meta" }, source.date),
      chevronIcon(),
    ),
    facts,
  );
}

// Links are derived from content (an explicit link, or an id in prose); the chain stores none. No section when both are empty.
function linksSection(links: TodoLinks): HTMLElement | null {
  const out = neighbourList(links.out);
  const back = neighbourList(links.in);
  if (out === null && back === null) return null;
  return el(
    "section",
    { class: "linksection" },
    el("h2", null, "Links"),
    out === null ? null : [el("h3", { class: "linkhead" }, `Links to (${links.out.length})`), out],
    back === null ? null : [el("h3", { class: "linkhead" }, `Linked from (${links.in.length})`), back],
    el(
      "p",
      { class: "legend" },
      'Derived from content — an explicit #link("todo:…"), or an id spelled in prose. The chain stores no link.',
    ),
  );
}

// A quoted string is prose and wraps; any other literal is structure, shown whole in its own scroller.
const printed = (v: string): HTMLElement =>
  v === "" ? el("span", { class: "meta" }, "—")
  : /^['"]/.test(v) ? el("span", { class: "prose" }, v)
  : el("code", { class: "literal" }, v);

// Only an `Authored` carries a content diff, so `undefined` is any other event.
function diffTable(changes: FieldChange[] | undefined): HTMLElement | null {
  if (changes === undefined) return null;
  if (changes.length === 0) return el("p", { class: "meta" }, "no content change against the previous Authored record");
  return el(
    "dl",
    { class: "diff" },
    changes.map((c) =>
      c.before === ""
        ? [el("dt", null, c.field), el("dd", { class: "now" }, printed(c.after))]
        : [
            el("dt", null, c.field),
            el("dd", { class: "was" }, printed(c.before)),
            el("dd", { class: "now" }, el("span", { class: "arrow" }, "→"), printed(c.after)),
          ],
    ),
  );
}

// `kindOfHash` names what an undo or a redo points at in the same stream.
function eventItem(link: Link, diffs: TodoPayload["diffs"], kindOfHash: (hash: string) => string | undefined): HTMLElement {
  const event = link.holds === "event" ? link : null;
  const act = event === null ? null : actLabel(event.reverses, kindOfHash);
  return el(
    "li",
    { class: "tlitem" },
    el(
      "div",
      { class: "tlhead" },
      el("strong", null, link.kind),
      act === null ? null : chip(act),
      event === null ? null : el("span", { class: "meta" }, `${iso(event.at)} · ${event.actor}`),
    ),
    diffTable(diffs[link.hash]),
    el("div", { class: "hashes" }, "hash ", hashLink(link.hash), link.parents.length === 0 ? " · genesis" : null),
  );
}

// The tab's cache of fetched pages: what a todo page shows when the box does not answer.
let tabHeld: reading.Held = reading.none;

export namespace todoview {
  // The page the box printed, or what this tab holds of it, and whether this login may write.
  export type T = { readonly got: reading.Got; readonly writes: boolean };
  type Parts = "acts" | "edit" | "news" | "proposed" | "curve";

  // The box's answer, or — when it gives none — what this tab already holds.
  const got = (id: string): Promise<reading.Got> =>
    fetchTodo(id).then(
      (payload): reading.Got => {
        tabHeld = reading.keep(tabHeld, payload);
        return { type: "Fetched", payload };
      },
      (error: unknown) => {
        const g = reading.fallback(tabHeld, onscreen.entries(store.state()), id, String(error));
        if (g === null) throw error;
        return g;
      },
    );

  // A todo's page, asked by its chain and id.
  // A page is the same page while what it draws is: the box's reading of it, its history and its number as a whole
  // percent, which a moment's drift below that leaves where it was.
  const drawn = (v: Entry["value"]): unknown => (typeof v === "number" ? price.percentOf(v) : v);
  const samePage = (a: reading.Got, b: reading.Got): boolean =>
    a.type === b.type &&
    reading.foldGot(a, {
      Fetched: (p) => b.type === "Fetched" && p.reading === b.payload.reading && p.stream.length === b.payload.stream.length &&
        p.stream[0]?.hash === b.payload.stream[0]?.hash && p.entry.state === b.payload.entry.state && data.same(drawn(p.entry.value), drawn(b.payload.entry.value)),
      Held: () => false,
      Unfetched: () => false,
    });
  const pages = asked.family((at: { readonly chain: string; readonly id: string }) => `${at.chain}/${at.id}`, (at) => got(at.id), samePage);
  const writes = readings.held(() => app.writes(app.now()), (moved) => app.hear(app.writes, moved));

  const unanswered = (g: reading.Got): string | null =>
    reading.foldGot(g, {
      Fetched: () => null,
      Held: (_, why) => `The box did not answer (${why}); this is the page as this tab last fetched it.`,
      Unfetched: (_, why) => `The box did not answer (${why}); this tab never fetched this todo's page.`,
    });

  const editable = (v: T): boolean => v.writes && reading.entryOf(v.got).authored !== null;

  // `applicable` decides the buttons, as it does on a card; Confirm is only here, beside the record it agrees with.
  // One note field serves the row, kept across a reload and cleared after an act that took, never one that bounced.
  const acts = (reload: () => void): frame.Island<T> => {
    const row = el("div", { class: "actrow" });
    const field = noteField();
    return {
      node: row,
      update: (v) => {
        const entry = reading.entryOf(v.got);
        const withClear: Acted = () => {
          field.value = "";
          reload();
        };
        const offered = act.offered<Node[]>(
          act.gate(v.writes),
          () => [...applicable(entry).map((name) => actButton(name, entry.todo, withClear, "", () => noteOf(field.value))), ...build(v), field],
          [],
        );
        // A login that may not write is offered nothing, and an empty row is not drawn.
        row.hidden = offered.length === 0;
        row.replaceChildren(...offered);
      },
    };
  };

  // An open todo of the chain sessions build files the card that would start one; its card is where it is tapped.
  const build = (v: T): Node[] => {
    const entry = reading.entryOf(v.got);
    const payload = reading.payloadOf(v.got);
    return payload === null || payload.builder === null || !isOpen(entry)
      ? []
      : [act.view({ label: "Build it", cls: "act act-build", run: () => buildTodo(entry.todo).then((p) => (address.go({ type: "conversation", id: p.id, refs: [] }), act.SPENT)) })];
  };

  // A half-typed edit survives a reload that brings the same content; new content is a new form.
  const edit = (reload: () => void): frame.Island<T> => {
    const node = el("div");
    let drawn = "";
    return {
      node,
      update: (v) => {
        const entry = reading.entryOf(v.got);
        const key = editable(v) ? JSON.stringify([entry.authored, isOpen(entry)]) : "";
        if (key === drawn) return;
        drawn = key;
        node.replaceChildren(
          ...(entry.authored === null || key === ""
            ? []
            : [editPanel(entry.todo, entry.authored, isUnconfirmed(entry), reload, () => app.now().categories), isOpen(entry) ? deferControl(entry.todo, reload) : ""]),
        );
      },
    };
  };

  // What the box refused of this tab's objects, redrawn on every replica commit.
  const news = (): frame.Island<T> => {
    const node = el("div");
    const draw = (): void => {
      const blocks = [
        refusalNotice((shown) => {
          void resetReplica(shown);
        }),
        waitingNotice(),
      ];
      node.replaceChildren(...blocks.filter((block) => block !== null));
    };
    const unwatch = watchReplica(draw);
    return { node, update: draw, destroy: unwatch };
  };

  // What is proposed against this todo, asked after each load so the todo draws without waiting on it.
  const proposed = (id: string): frame.Island<T> => {
    const node = el("div");
    let destroyed = false;
    return {
      node,
      update: () =>
        void dressed
          .inbox(time.moment(), null)
          .then(({ payload }) => {
            if (destroyed) return;
            const standing = payload.proposals.filter((p) => p.state.kind === "open" && p.serves?.endsWith(`/${id}`) === true);
            node.replaceChildren(
              ...(standing.length === 0
                ? []
                : [
                    el(
                      "p",
                      { class: "proposednote", role: "status" },
                      `${standing.length} proposed change${standing.length === 1 ? "" : "s"} to this todo ${standing.length === 1 ? "is" : "are"} waiting in the `,
                      el("a", { href: hrefOf({ type: "inbox" }) }, "Inbox"),
                      ". Nothing here reflects it until you tap it.",
                    ),
                  ]),
            );
          })
          // A todo page that cannot read the Inbox is still a todo page.
          .catch(() => {}),
      destroy: () => {
        destroyed = true;
      },
    };
  };

  // The thirty days, sampled by the core in this tab from the objects it holds at the fold's instant, so it is a view of
  // the reading and moves when the reading does (a commit, or the clock reaching the fold's next change).
  const curveBox = (id: string): frame.Island<T> => {
    const soon = el("div", { class: "curvebox" });
    let span: thirty.Thirty = thirty.sampling;
    let destroyed = false;
    const say = (s: string): HTMLElement => el("p", { class: "legend" }, s);
    // One node for the whole wait, so drawing it again does not start its 300 ms again.
    const sampling = placeholder.view(placeholder.LINES);
    const paint = (): void => {
      const next = thirty.fold(span, {
        Sampling: () => sampling,
        Unheld: () => say("No curve yet: this browser does not hold this todo's events. It draws once they sync."),
        Unpriced: () => say("No curve: nothing recorded by now gives this todo a spec or a checklist, so it has no function to draw."),
        Drawn: (s) => curve.fold(s, { Unsampled: () => sampling, Sampled: (d) => curve.line(d, app.now().zone) }),
      });
      if (soon.childNodes.length !== 1 || soon.firstChild !== next) soon.replaceChildren(next);
    };
    // Each sample is numbered so one landing late never overwrites a later one; the open chain and the others are read at
    // one instant.
    let asked = 0;
    const resample = (): void => {
      const mine = ++asked;
      // The chain this sample reads, taken before any wait, so a switch mid-read cannot file it under the next.
      const on = app.now().chain;
      // Sampled by the core in its worker, from the chains it holds: nothing here reads a chain.
      void askHeld(() => ({ type: "Thirty", chain: on, id, ms: readAt() }), "Page")
        .then(({ answer }) => {
          if (destroyed || mine !== asked) return;
          span = answer;
          paint();
        })
        .catch((error: unknown) => soon.replaceChildren(el("p", { class: "problem" }, `No curve: ${String(error)}`)));
    };
    const readAt = (): number => {
      const at = onscreen.at(store.state());
      return (at === null ? null : time.ms(at)) ?? time.now();
    };
    let at = onscreen.at(store.state());
    const moved = (f: onscreen.Fold): void => {
      if (onscreen.at(f) === at) return;
      at = onscreen.at(f);
      resample();
    };
    const unwatch = [watchReplica(resample), watchOthers(resample), store.subscribe(moved)];
    return {
      node: el("section", { class: "pricebox" }, el("h2", { class: "priceheading" }, "Fulfillment, 30 days"), soon),
      update: () => (paint(), resample()),
      destroy: () => {
        destroyed = true;
        unwatch.forEach((u) => u());
      },
    };
  };

  // Closed until asked for: the record of every change, its literals and hashes, is evidence and rarely read.
  const history = (payload: TodoPayload): Child => {
    const kindOfHash = (hash: string): string | undefined => payload.stream.find((l) => l.hash === hash)?.kind;
    return [
      linksSection(payload.links),
      disclosure.details(
        disclosure.of(ui.cell(registers.told, `${payload.todo}/history`)),
        { class: "history" },
        [el("h2", null, `History (${payload.stream.length} ${payload.stream.length === 1 ? "event" : "events"})`), chevronIcon()],
        el("p", { class: "legend" }, "Oldest first. Tap a hash to check that object on the Chain page."),
        el("ol", { class: "timeline" }, payload.stream.map((link) => eventItem(link, payload.diffs, kindOfHash))),
      ),
    ];
  };

  // `back` is the pushed screen's; a pane beside the list it came from has nothing to go back to.
  export const page = (id: string, back: boolean): frame.Page<T, never, Parts> => ({
    className: "todoview",
    // No title until the todo is read: its id is not its name.
    title: "",
    ...(back ? { back: { label: "Todos", fallback: { type: "todos" } } } : {}),
    reads: () => readings.map(readings.both(pages({ chain: app.now().chain, id }), writes), ([g, w]) => ({ got: g, writes: w })),
    // The act row was built before `GET /api/me` answered, so a login that may write is told by a reload.
    listen: (_, reload) => app.hear(app.actor, reload),
    islands: (_, reload) => ({ acts: acts(reload), edit: edit(reload), news: news(), proposed: proposed(id), curve: curveBox(id) }),
    masthead: (v) => {
      const said = unanswered(v.got);
      return [
        said === null ? null : el("p", { class: "legend offlinenote", role: "status" }, said),
        marks.head(...reading.view(reading.readingOf(v.got)).head),
      ];
    },
    // What it says, what you can do about it, what else it says, then who says so.
    view: (v, _, reload, at) => {
      const entry = reading.entryOf(v.got);
      const payload = reading.payloadOf(v.got);
      return [
        at.acts,
        at.edit,
        el("div", { class: "reading" }, ...reading.view(reading.readingOf(v.got)).rest),
        content(entry, editable(v), reload),
        // The chain is part of the reference because an id is only unique within one store.
        el("div", { class: "askrow" }, askLink({ kind: "todo", chain: app.now().chain, id: entry.todo })),
        provenance(entry),
        conflictsBlock(entry),
        at.news,
        claimNote(entry),
        at.proposed,
        at.curve,
        payload === null ? el("p", { class: "legend" }, "Its links and history come with its page from the box.") : history(payload),
      ];
    },
  });
}
export default todoview;
