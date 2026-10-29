// One mail thread, oldest first, with only the newest open: read the way it happened. Bodies are text.

import { mailThread, revealHeld } from "../wire/api";
import heldvalue from "../view/heldvalue";
import type { MailMessage, ThreadPayload } from "../wire/generated/wire";
import { el } from "../view/dom";
import { chip, whenOf } from "../view/format";
import { chevronIcon } from "../view/icons";
import { badgeOf, senderOf, subjectOf, todoOf } from "./mailrow";
import type { Mounted } from "../view/page";
import { askLink } from "../chat/refs";
import { hrefOf } from "../shell/route";
import frame from "../view/frame";
import placeholder from "../view/placeholder";
import asked from "../app/asked";
import app from "../app/app";

/** A longer thread comes back as its newest messages: the end, the part being read. */
const LIMIT = 100;

function messageBlock(message: MailMessage, open: boolean): HTMLElement {
  const row = message.row;
  const badge = badgeOf(row.disposition);
  const todo = todoOf(row.disposition);
  return el(
    "details",
    { class: "why threadmsg", open },
    el(
      "summary",
      null,
      el("span", { class: "threadwho" }, senderOf(row)),
      el("span", { class: "meta" }, whenOf(row.date, app.now().zone)),
      chevronIcon(),
    ),
    el(
      "div",
      { class: "mailchips" },
      row.account === "" ? null : chip(row.account, "cat"),
      badge === null ? null : chip(badge.label, badge.kind),
      todo === null ? null : el("code", { class: "cardid" }, todo),
      el("a", { class: "hashlink", href: hrefOf({ type: "message", id: row.id }) }, "open"),
    ),
    heldvalue.within(el("div", { class: "mailbody" }, message.body === "" ? "(this message has no readable body)" : message.body), revealHeld(null)),
    message.truncated ? el("p", { class: "legend" }, "Cut at 200 KB.") : null,
  );
}

const threads = asked.family((id: string) => id, (id: string) => mailThread(id, LIMIT));

export function mount(root: HTMLElement, id: string): Promise<Mounted> {
  return frame.mount<ThreadPayload>(root, {
    className: "mailthread",
    title: (thread) => {
      const first = thread?.messages[0];
      return first === undefined ? "Conversation" : subjectOf(first.row);
    },
    back: { label: "Mail", fallback: { type: "mail", query: "", account: "" } },
    legend: (thread) =>
      `${thread.messages.length} message${thread.messages.length === 1 ? "" : "s"}, oldest first · notmuch thread ${thread.thread}`,
    reads: () => threads(id),
    shape: placeholder.ROWS,
    // Ask about the whole conversation, the reference only this page can offer.
    view: (thread) => [
      el("div", { class: "askrow" }, askLink({ kind: "thread", id: thread.thread })),
      thread.messages.map((message, index) => messageBlock(message, index === thread.messages.length - 1)),
    ],
  });
}
