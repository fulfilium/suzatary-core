// One message: its body as text (mail is never markup here, and never goes near rich.ts), its links, and the todos
// whose latest record names it, drawn as the list's own cards with their acts.

import { mailMessage, revealHeld } from "../wire/api";
import heldvalue from "../view/heldvalue";
import http from "../wire/http";
import app from "../app/app";
import type { Entry, MailMessage } from "../wire/generated/wire";
import { cardList } from "../todo/card";
import type { Child } from "../view/dom";
import { el } from "../view/dom";
import { chip, whenOf } from "../view/format";
import { chevronIcon } from "../view/icons";
import { badgeOf, subjectOf } from "./mailrow";
import type { Mounted } from "../view/page";
import { askLink } from "../chat/refs";
import { hrefOf } from "../shell/route";
import onscreen, { store } from "../app/store";
import { DEFAULT_FILTER, listing } from "../todo/todos";
import frame from "../view/frame";
import asked from "../app/asked";
import time from "../app/time";

function chips(message: MailMessage): HTMLElement {
  const row = message.row;
  const badge = badgeOf(row.disposition);
  return el(
    "div",
    { class: "mailchips" },
    row.account === "" ? null : chip(row.account, "cat"),
    chip(row.container, "kind"),
    badge === null ? null : chip(badge.label, badge.kind),
    message.thread === ""
      ? null
      : el(
          "a",
          { class: "threadlink", href: hrefOf({ type: "thread", id: message.thread }) },
          "Whole conversation",
          chevronIcon(),
        ),
  );
}

// Collapsed: open, it would repeat the title and the line under it and push the body below a phone's fold.
function headerTable(message: MailMessage): HTMLElement {
  const row = message.row;
  return el(
    "details",
    { class: "why" },
    el("summary", null, "Headers", chevronIcon()),
    el(
      "dl",
      { class: "facts mailfacts" },
      message.headers.map((header) => [el("dt", null, header.name.toLowerCase()), el("dd", null, header.value)]),
      el("dt", null, "account"),
      el("dd", null, row.account === "" ? "—" : row.account),
      el("dt", null, "folder"),
      el("dd", null, row.container),
      el("dt", null, "hash"),
      el("dd", null, el("code", { class: "hash", title: row.hash }, row.hash.slice(0, 16))),
      el("dt", null, "id"),
      el("dd", null, el("code", { class: "mailid" }, row.id)),
    ),
  );
}

// Every URL is shown beside its label, because a label is the sender's claim about where a link goes.
function linkList(message: MailMessage): HTMLElement | null {
  if (message.links.length === 0) return null;
  return el(
    "div",
    null,
    el("h2", null, `Links (${message.links.length})`),
    el(
      "ul",
      { class: "maillinks" },
      message.links.map((link) =>
        el(
          "li",
          null,
          link.label === "" ? null : el("span", { class: "linklabel" }, link.label),
          // Clamped by CSS, never cut: cutting would be this app deciding what a link says.
          el(
            "a",
            { href: link.url, rel: "noopener noreferrer", target: "_blank", class: "linkurl", title: link.url },
            link.url,
          ),
        ),
      ),
    ),
  );
}

/** An act re-reads the todos from the store; the actor arriving redraws their buttons. */
type Own = "acted" | "actor";

/** The message, or the box's words for one it no longer keeps, whatever still cites it. */
type Read = { type: "Found"; message: MailMessage } | { type: "Gone"; said: string };

function stepOwn(read: Read, event: Own): Read {
  if (read.type === "Gone") return read;
  const message = read.message;
  if (event === "actor") return { type: "Found", message: { ...message } };
  return {
    type: "Found",
    message: { ...message, todos: message.todos.map((entry) => onscreen.entry(store.state(), entry.todo) ?? entry) },
  };
}

/** A 404 is a fact about the maildir (its account removed with its mail, or never synced), drawn as one, never a Retry. */
async function found(id: string): Promise<Read> {
  try {
    return { type: "Found", message: await mailMessage(id) };
  } catch (error: unknown) {
    if (error instanceof http.ApiError && error.status === 404) return { type: "Gone", said: error.message };
    throw error;
  }
}

const gone = (id: string, said: string): Child => [
  el("code", { class: "mailid" }, id),
  el("p", { class: "legend mailgone" }, `${said.charAt(0).toUpperCase()}${said.slice(1)}.`),
  el("p", { class: "legend" }, "The sync window is INBOX + 7 days; anything older or elsewhere needs a backfill."),
];

function todosFrom(todos: readonly Entry[], send: (e: Own) => void): Child {
  const cards = cardList({ acted: () => send("acted") }, "This message produced no todos.");
  // "all", not "open": hiding the done ones would make a source look like it produced nothing.
  const shown = listing(todos, { ...DEFAULT_FILTER, state: "all" }, time.moment());
  cards.update(shown.priced, shown.unpriced);
  return [
    el("h2", null, `Todos from this mail (${todos.length})`),
    el(
      "p",
      { class: "legend" },
      todos.length === 0
        ? "No todo on the chain names this message as its source."
        : "Every todo whose latest record names this message. The numbers are today's.",
    ),
    cards.node,
  ];
}

const messages = asked.family((id: string) => id, found);

export function mount(root: HTMLElement, id: string): Promise<Mounted> {
  return frame.mount<Read, Own>(root, {
    className: "mailmessage",
    title: (read) => (read === null || read.type === "Gone" ? "Message" : subjectOf(read.message.row)),
    back: { label: "Mail", fallback: { type: "mail", query: "", account: "" } },
    legend: (read) =>
      read.type === "Gone"
        ? ""
        : `${read.message.row.sender}${read.message.row.date === "" ? "" : ` · ${whenOf(read.message.row.date, app.now().zone)}`}`,
    reads: () => messages(id),
    step: stepOwn,
    // These cards carry a Done, so they are drawn again once `GET /api/me` says whether this login may write.
    listen: (send) => app.hear(app.actor, () => send("actor")),
    // The body is a stranger's prose and `el` sets it by textContent; the ask names the canonical hash this page is keyed on.
    view: (read, send) =>
      read.type === "Gone"
        ? gone(id, read.said)
        : [
            chips(read.message),
            el("div", { class: "askrow" }, askLink({ kind: "mail", hash: read.message.row.hash })),
            heldvalue.within(el("div", { class: "mailbody" }, read.message.body === "" ? "(this message has no readable body)" : read.message.body), revealHeld(null)),
            read.message.truncated
              ? el("p", { class: "legend" }, "Cut at 200 KB — the rest is in the maildir, not on this page.")
              : null,
            linkList(read.message),
            todosFrom(read.message.todos, send),
            headerTable(read.message),
          ],
  });
}
