// The palette's one result shape, and the pure adapters from mail and conversation rows into it.

import type { ConversationRow } from "../wire/generated/wire";
import type { MailRow } from "../wire/generated/wire";
import type { Route } from "./route";

export type HitKind = "destination" | "command" | "todo" | "message" | "conversation";

/** `search` is what `match.ts` ranks against; `title`/`subtitle` are what a row draws. */
export interface Hit {
  readonly kind: HitKind;
  readonly search: string;
  readonly title: string;
  readonly subtitle?: string;
  readonly route: Route;
}

/** One mail message: the subject, the sender to tell same-subject messages apart, and its `#/mail/<id>` route. */
export function hitOfMail(row: MailRow): Hit {
  return {
    kind: "message",
    search: `${row.subject} ${row.sender}`,
    title: row.subject,
    subtitle: row.sender,
    route: { type: "message", id: row.id },
  };
}

/** One conversation whose title the device holds; `turns` is the subtitle, the one fact a title does not carry. A row
 * whose words have not landed has no title to match, so it is no hit until they do. */
export function hitOfConversation(row: ConversationRow): Hit[] {
  if (row.words.held !== "read") return [];
  return [
    {
      kind: "conversation",
      search: row.words.title,
      title: row.words.title,
      subtitle: row.turns === 1 ? "1 turn" : `${row.turns} turns`,
      route: { type: "conversation", id: row.id, refs: [] },
    },
  ];
}

/** One letter matches almost everything, so a query under two characters asks no server. */
export function needsAsync(query: string): boolean {
  return query.trim().length >= 2;
}
