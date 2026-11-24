// The keyboard's shell: one listener, the `g` prefix, and the page listening; `keys.ts` decides what a key means.

import { el } from "../view/dom";
import type { KeyAction } from "./keys";
import { SHORTCUTS, keyAction } from "./keys";
import { closeOverlay, openOverlayName, overlayOpen, showOverlay } from "../view/overlay";
import app from "../app/app";
import type { ActionName } from "../todo/todos";
import address from "./address";

/** What a page can answer; a page with no cursor lets the key fall through to nothing rather than a guess. */
export interface KeyTarget {
  readonly move?: (by: number) => void;
  readonly open?: () => void;
  readonly close?: () => void;
  readonly act?: (name: ActionName) => void;
  /** Undo/redo the last event on whatever this page calls "focused". */
  readonly undo?: () => void;
  readonly redo?: () => void;
  readonly focusSearch?: () => void;
  readonly add?: () => void;
  /** True when the page consumed the Escape (it closed its own sheet, say). */
  readonly escape?: () => boolean;
}

const NOBODY: KeyTarget = {};
let target: KeyTarget = NOBODY;

export function setKeyTarget(next: KeyTarget): void {
  target = next;
}

/** Only the page that registered may clear it, so a page destroyed late cannot take its successor's keyboard. */
export function clearKeyTarget(mine: KeyTarget): void {
  if (target === mine) target = NOBODY;
}

/** Whether no page holds the keys: the one a page drawn again may take them back from. */
export function keysFree(): boolean {
  return target === NOBODY;
}

/** What the current page answers; the palette offers exactly this. */
export function keyTarget(): KeyTarget {
  return target;
}

// Registered rather than imported: `palette.ts` reads this module's target, so importing it back would be a cycle.
let palette: (() => void) | null = null;

export function setPaletteOpener(open: () => void): void {
  palette = open;
}

function typing(node: Element | null): boolean {
  if (node === null) return false;
  if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement || node instanceof HTMLSelectElement) {
    return true;
  }
  return node instanceof HTMLElement && node.isContentEditable;
}

const SHORTCUT_SHEET = "shortcuts";

/** The shortcuts, from the same table the handler switches on. */
export function toggleShortcuts(): void {
  if (openOverlayName() === SHORTCUT_SHEET) {
    closeOverlay();
    return;
  }
  showOverlay(
    SHORTCUT_SHEET,
    "Keyboard shortcuts",
    el(
      "div",
      { class: "shortcuts" },
      el("h2", { class: "overlaytitle" }, "Keyboard"),
      el(
        "dl",
        { class: "shortcutlist" },
        SHORTCUTS.map((row) => [el("dt", null, el("kbd", null, row.keys)), el("dd", null, row.what)]),
      ),
      el("p", { class: "legend" }, "Keys are ignored while a field has the keyboard — except Escape, which leaves it."),
    ),
  );
}

/** The prefix expires, so a stray `g` cannot swallow a key pressed a minute later. */
const PREFIX_MS = 1500;

function run(action: KeyAction): boolean {
  switch (action.type) {
    case "move":
      if (target.move === undefined) return false;
      target.move(action.by);
      return true;
    case "open":
      if (target.open === undefined) return false;
      target.open();
      return true;
    case "close":
      if (target.close === undefined) return false;
      target.close();
      return true;
    case "act":
      if (target.act === undefined) return false;
      target.act(action.name);
      return true;
    case "undo":
      if (target.undo === undefined) return false;
      target.undo();
      return true;
    case "redo":
      if (target.redo === undefined) return false;
      target.redo();
      return true;
    case "focusSearch":
      if (target.focusSearch === undefined) return false;
      target.focusSearch();
      return true;
    case "add":
      if (target.add === undefined) return false;
      target.add();
      return true;
    case "escape":
      // The top thing open is what Escape means, whatever page is under it.
      if (overlayOpen()) return closeOverlay();
      return target.escape?.() ?? false;
    case "help":
      toggleShortcuts();
      return true;
    case "palette":
      if (palette === null) return false;
      palette();
      return true;
    case "go":
      address.go(action.route);
      return true;
    case "arm":
      return true;
  }
}

export function installKeyboard(): void {
  let pending = "";
  let expiry = 0;
  document.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.isComposing) return;
    const action = keyAction(event.key, {
      typing: typing(document.activeElement),
      pending,
      modified: event.ctrlKey || event.metaKey || event.altKey,
      onConversation: app.route(app.now()).type === "conversation",
    });
    window.clearTimeout(expiry);
    pending = "";
    if (action === null) return;
    if (action.type === "arm") {
      pending = "g";
      expiry = window.setTimeout(() => {
        pending = "";
      }, PREFIX_MS);
    }
    // A key nobody answered stays the browser's: `/` with no search is its quick find.
    if (run(action)) event.preventDefault();
  });
}
