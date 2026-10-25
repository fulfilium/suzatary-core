// ⌘K: every destination, todo, message and conversation in one ranked list. Local rows answer at once;
// mail and conversations are appended below under a label, never reordering what is shown, and a
// failed source draws one quiet row. "Ask the secretary" is built here, not in the pure `match.ts`.

import { mailSearch } from "../wire/api";
import conversations from "../chat/conversations";
import lo from "../algebra/lo";
import { el } from "../view/dom";
import { hitOfConversation, hitOfMail, needsAsync } from "./hits";
import type { Hit, HitKind } from "./hits";
import { keyTarget, toggleShortcuts } from "./keyboard";
import { rank } from "../algebra/match";
import nav from "./nav";
import { closeOverlay, showOverlay } from "../view/overlay";
import type { Route } from "./route";
import onscreen, { store } from "../app/store";
import { headline } from "../todo/todos";
import address from "./address";

interface Command {
  readonly kind: HitKind;
  /** What the query is matched against: the label plus anything worth finding it by. */
  readonly search: string;
  readonly label: string;
  readonly hint: string;
  readonly run: () => void;
  /** Where a "go" command leads, so a node test can assert it without a `window`. */
  readonly route?: Route;
}

/** An async `Hit`, drawn as a `Command` with no local `run` of its own. */
function commandOfHit(hit: Hit): Command {
  return {
    kind: hit.kind,
    search: hit.search,
    label: hit.title,
    hint: hit.subtitle ?? "",
    route: hit.route,
    run: () => {
      address.go(hit.route);
    },
  };
}

/** The one quiet row a failed async source draws; it routes nowhere. */
function unavailableRow(label: string): Command {
  return { kind: "command", search: "", label, hint: "", run: () => {} };
}

const DEBOUNCE_MS = 250;
const ASYNC_LIMIT = 5;

/** `#/secretary` focused with no reference, where `a` and this entry both go. */
const ASK_ANYWHERE: Route = { type: "secretary", refs: [], topics: [], focus: true };

const MOST = 12;

export function commands(): Command[] {
  const target = keyTarget();
  const out: Command[] = nav.DESTINATIONS.map((tab) => ({
    kind: "destination",
    search: `go ${tab.label}`,
    label: tab.label,
    hint: "go",
    route: tab.route,
    run: () => {
      address.go(tab.route);
    },
  }));
  out.push({
    kind: "command",
    search: "ask the secretary anywhere question",
    label: "Ask the secretary",
    hint: "ask",
    route: ASK_ANYWHERE,
    run: () => {
      address.go(ASK_ANYWHERE);
    },
  });
  if (target.add !== undefined) {
    out.push({
      kind: "command",
      search: "new todo add",
      label: "New todo",
      hint: "add",
      run: () => target.add?.(),
    });
  }
  if (target.focusSearch !== undefined) {
    out.push({
      kind: "command",
      search: "search todos filter",
      label: "Search this list",
      hint: "search",
      run: () => target.focusSearch?.(),
    });
  }
  out.push({
    kind: "command",
    search: "keyboard shortcuts help",
    label: "Keyboard shortcuts",
    hint: "help",
    run: toggleShortcuts,
  });
  for (const entry of onscreen.entries(store.state())) {
    out.push({
      kind: "todo",
      search: `${entry.todo} ${headline(entry)}`,
      label: headline(entry),
      hint: entry.todo,
      run: () => {
        address.go({ type: "todo", id: entry.todo });
      },
    });
  }
  return out;
}

/** One group of rows, `label === null` for the local one that renders first. */
interface Section {
  readonly label: string | null;
  readonly rows: Command[];
}

export function openPalette(): void {
  const localAll = commands();
  const field = el("input", {
    type: "text",
    class: "draftfield palettefield",
    placeholder: "Go to a page, or find a todo",
    "aria-label": "command palette",
    autocomplete: "off",
    spellcheck: "false",
  });
  const results = el("div", { class: "palettelist", role: "listbox", "aria-label": "results" });
  // The conversations' reading is heard while the palette is open and let go when it closes.
  let unhear = (): void => {};
  const box = showOverlay(
    "palette",
    "Command palette",
    el(
      "div",
      { class: "palette" },
      field,
      results,
      el("p", { class: "legend" }, "↑ ↓ to move, Enter to go, Escape to close."),
    ),
    () => unhear(),
  );

  // Escape and a scrim click close the overlay from outside, so every async callback checks this first: this palette is
  // open while its own field is on the page, whatever palette is open after it.
  const stillOpen = (): boolean => field.isConnected;

  let shown: Command[] = [];
  let at = 0;

  // Mail: the server's search, asked on every debounced keystroke; each answer is kept by the query it answers, and
  // the field's query reads its own, so an answer landing late is a row of the table, never the one shown.
  const mail = new Map<string, Command[] | "failed">();
  let debounceHandle: ReturnType<typeof window.setTimeout> | undefined;

  // Conversations: the replica's reading, heard while the palette is open, so one it takes in meanwhile is found.
  const convos = conversations.rows;

  const sections = (): Section[] => {
    const query = field.value;
    const local = rank(query, localAll, (command) => command.search, MOST);
    const out: Section[] = [{ label: null, rows: local }];
    if (!needsAsync(query)) return out;
    const mailRows = mail.get(query) ?? [];
    if (mailRows === "failed") out.push({ label: "Mail", rows: [unavailableRow("Mail search unavailable")] });
    else if (mailRows.length > 0) out.push({ label: "Mail", rows: mailRows });
    const convoRows = lo.fold(lo.flatten(convos.now()), {
      loading: (): Command[] => [],
      error: () => [unavailableRow("Conversations unavailable")],
      val: (rows) => rank(query, rows.map(hitOfConversation), (hit) => hit.search, ASYNC_LIMIT).map(commandOfHit),
    });
    if (convoRows.length > 0) out.push({ label: "Conversations", rows: convoRows });
    return out;
  };

  const draw = (): void => {
    const secs = sections();
    shown = secs.flatMap((section) => section.rows);
    at = Math.min(at, Math.max(0, shown.length - 1));
    const nodes: HTMLElement[] = [];
    let index = 0;
    for (const section of secs) {
      if (section.label !== null) nodes.push(el("p", { class: "legend palgroup" }, section.label));
      for (const command of section.rows) {
        const here = index;
        const row = el(
          "button",
          {
            type: "button",
            class: here === at ? "palrow on" : "palrow",
            role: "option",
            "aria-selected": here === at ? "true" : "false",
          },
          el("span", { class: "pallabel" }, command.label),
          el("span", { class: "palhint" }, command.hint),
        );
        row.addEventListener("click", () => {
          closeOverlay();
          command.run();
        });
        nodes.push(row);
        index += 1;
      }
    }
    results.replaceChildren(...nodes);
    if (shown.length === 0) results.appendChild(el("p", { class: "legend" }, "Nothing matches."));
  };

  const scheduleMail = (query: string): void => {
    if (debounceHandle !== undefined) window.clearTimeout(debounceHandle);
    if (!needsAsync(query) || mail.has(query)) return;
    debounceHandle = window.setTimeout(() => {
      void mailSearch(query, "", ASYNC_LIMIT)
        .then((payload) => {
          if (!stillOpen()) return;
          mail.set(query, rank(query, payload.messages.map(hitOfMail), (hit) => hit.search, ASYNC_LIMIT).map(commandOfHit));
          draw();
        })
        .catch(() => {
          if (!stillOpen()) return;
          mail.set(query, "failed");
          draw();
        });
    }, DEBOUNCE_MS);
  };

  field.addEventListener("input", () => {
    at = 0;
    scheduleMail(field.value);
    draw();
  });
  field.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (shown.length === 0) return;
      at = (at + (event.key === "ArrowDown" ? 1 : shown.length - 1)) % shown.length;
      draw();
      return;
    }
    if (event.key !== "Enter") return;
    event.preventDefault();
    const chosen = shown[at];
    if (chosen === undefined) return;
    closeOverlay();
    chosen.run();
  });

  draw();
  // The field takes the keyboard immediately.
  field.focus();
  box.scrollTop = 0;

  unhear = convos.hear(draw);
}
