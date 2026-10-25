// What a key means where it is pressed, a pure function of (key, context). A focused field keeps every key but
// Escape, a modified key is the browser's except ⌘K, and `g` arms a destination the next key names.

import type { Route } from "./route";
import type { ActionName } from "../todo/todos";

export type KeyAction =
  /** Move the list's cursor by ±1. */
  | { readonly type: "move"; readonly by: number }
  /** Open whatever the cursor is on. */
  | { readonly type: "open" }
  /** Close it again: the in-place todo, the fold, then up to the parent. */
  | { readonly type: "close" }
  | { readonly type: "act"; readonly name: ActionName }
  /** Not an "act": undo and redo apply in any state, and only the box refuses them. */
  | { readonly type: "undo" }
  | { readonly type: "redo" }
  | { readonly type: "focusSearch" }
  | { readonly type: "add" }
  /** Close the top thing that is open, or clear the cursor if nothing is. */
  | { readonly type: "escape" }
  | { readonly type: "help" }
  | { readonly type: "palette" }
  /** The `g` prefix was pressed; the next key is a destination. */
  | { readonly type: "arm" }
  | { readonly type: "go"; readonly route: Route };

export interface KeyContext {
  /** An input, textarea, select or contenteditable has the keyboard. */
  readonly typing: boolean;
  /** "g" when the prefix is armed, "" otherwise. */
  readonly pending: string;
  /** Ctrl, Cmd or Alt is held. */
  readonly modified: boolean;
  /** Already on the conversation `a` would open; absent reads as not. */
  readonly onConversation?: boolean;
}

/** `g <key>`: each destination by its tab's first letter. */
const DESTINATIONS: Record<string, Route> = {
  t: { type: "todos" },
  i: { type: "inbox" },
  p: { type: "project" },
  m: { type: "mail", query: "", account: "" },
  s: { type: "secretary", refs: [], topics: [] },
  b: { type: "briefings" },
};

export function keyAction(key: string, context: KeyContext): KeyAction | null {
  // The palette works while typing, so you can leave a field without deciding where first.
  if (context.modified) return key === "k" || key === "K" ? { type: "palette" } : null;
  if (context.typing) return key === "Escape" ? { type: "escape" } : null;
  if (context.pending === "g") {
    const route = DESTINATIONS[key.toLowerCase()];
    return route === undefined ? null : { type: "go", route };
  }
  switch (key) {
    case "j":
      return { type: "move", by: 1 };
    case "k":
      return { type: "move", by: -1 };
    case "Enter":
    case "l":
      return { type: "open" };
    case "h":
      return { type: "close" };
    case "c":
      return { type: "act", name: "done" };
    case "x":
      return { type: "act", name: "cancel" };
    case "a":
      // Nothing on the conversation page itself: the compose box `a` would focus is already on screen.
      return context.onConversation === true
        ? null
        : { type: "go", route: { type: "secretary", refs: [], topics: [], focus: true } };
    case "u":
      return { type: "undo" };
    // Shift alone is not a modifier `context.modified` reserves, so `key` is already uppercase.
    case "U":
      return { type: "redo" };
    case "/":
      return { type: "focusSearch" };
    case "n":
      return { type: "add" };
    case "Escape":
      return { type: "escape" };
    case "?":
      return { type: "help" };
    case "g":
      return { type: "arm" };
    default:
      return null;
  }
}

/** What the shortcuts sheet lists: the same table, so the help and the behaviour cannot drift. */
export const SHORTCUTS: readonly { readonly keys: string; readonly what: string }[] = [
  { keys: "j / k", what: "move down / up the list" },
  { keys: "l / Enter", what: "open what is under the cursor: a fold, then the todo in place" },
  { keys: "h", what: "close it again, then climb to the group" },
  { keys: "a", what: "ask the secretary, from anywhere" },
  { keys: "c", what: "mark it done" },
  { keys: "x", what: "cancel it" },
  { keys: "u", what: "undo its last event" },
  { keys: "U", what: "redo the event an undo just undid" },
  { keys: "/", what: "search" },
  { keys: "n", what: "add a todo" },
  { keys: "⌘K / Ctrl-K", what: "the command palette" },
  { keys: "g t / g i / g s / g b", what: "todos / inbox / secretary / briefings — the bar" },
  { keys: "g p / g m", what: "project / mail" },
  { keys: "Escape", what: "close a sheet, or clear the cursor" },
  { keys: "?", what: "this list" },
];
