// The pure pieces of undo/redo: how long a toast offers the append, and how a stream line labels one.
// The chain never rewrites, so an undo is an appended inverse, and which event to build is the box's.

import type { Reverses } from "../wire/generated/wire";

export const UNDO_WINDOW_MS = 8000;

/** Whether an undo offered at `shownAt` is still live: a click that raced the toast's timer fires nothing. */
export function undoStillLive(shownAt: number, now: number): boolean {
  return now - shownAt < UNDO_WINDOW_MS;
}

export type KindOf = (hash: string) => string | undefined;

/**
 * What one stream line says about an undo or a redo, `null` for an ordinary event. Redo stays generic
 * ("redid"): it re-establishes what the undo took away, so its kind already says what happened.
 */
export function actLabel(reverses: Reverses, kindOf: KindOf): string | null {
  switch (reverses.kind) {
    case "nothing":
      return null;
    case "redo":
      return "redid";
    case "undo":
      return undid(kindOf(reverses.of));
  }
}

function undid(kind: string | undefined): string {
  switch (kind) {
    case "Completed":
      return "undid completion";
    case "Cancelled":
      return "undid cancellation";
    case "Reopened":
      return "undid reopening";
    case "SpecRevised":
      return "undid a price change";
    case "Authored":
      return "undid a content edit";
    default:
      // The named hash is not among those looked up; still an undo.
      return "undid the previous act";
  }
}
