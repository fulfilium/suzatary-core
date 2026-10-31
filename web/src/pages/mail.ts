// Mail by date, or any notmuch query: triage's reading is a badge, never a filter, so what it dismissed stays visible.
// The query is in the hash, so a search is a link and a navigation. Every string is attacker-controlled and set as text.

import { inbox as fetchInbox, mailSearch } from "../wire/api";
import type { MailRow, MailWatch } from "../wire/generated/wire";
import empty from "../view/empty";
import { el } from "../view/dom";
import { chip, whenOf } from "../view/format";
import { chevronIcon } from "../view/icons";
import heldvalue from "../view/heldvalue";
import { badgeOf, cursorOf, senderOf, subjectOf, todoOf } from "./mailrow";
import type { Mounted } from "../view/page";
import { hrefOf } from "../shell/route";
import frame from "../view/frame";
import placeholder from "../view/placeholder";
import asked from "../app/asked";
import reading from "../algebra/reading";
import app from "../app/app";
import address from "../shell/address";
import watches from "./watches";

/** The server's own cap (`MAX_MAIL`), so "Load more" asks for a page. */
const PAGE = 50;

// The whole row is an honest `<a>`: unlike a card, it holds no buttons.
function mailRow(row: MailRow): HTMLElement {
  const badge = badgeOf(row.disposition);
  const todo = todoOf(row.disposition);
  return el(
    "a",
    { class: "mailrow", href: hrefOf({ type: "message", id: row.id }) },
    el(
      "div",
      { class: "mailmeta" },
      el("span", { class: "mailsender", title: row.sender }, senderOf(row)),
      el("span", { class: "mailwhen" }, whenOf(row.date, app.now().zone)),
    ),
    el("div", { class: "mailsubject" }, subjectOf(row)),
    el(
      "div",
      { class: "mailchips" },
      row.account === "" ? null : chip(row.account, "cat"),
      row.container === "inbox" ? null : chip(row.container, "kind"),
      badge === null ? null : chip(badge.label, badge.kind),
      todo === null ? null : el("code", { class: "cardid" }, todo),
      ...heldvalue.named(row.held).map((said) => chip(said, "kind")),
    ),
  );
}

// From the server's roster, not the rows received: a filter offering only what is shown cannot find what is not.
function accountChips(accounts: readonly string[], active: string, query: string): HTMLElement {
  const link = (value: string, label: string): HTMLElement =>
    el(
      "a",
      {
        class: value === active ? "chipbtn on" : "chipbtn",
        href: hrefOf({ type: "mail", query, account: value }),
        "aria-current": value === active ? "true" : null,
      },
      label,
    );
  return el("div", { class: "chiprow" }, link("", "All"), ...accounts.map((account) => link(account, account)));
}

interface Listing {
  readonly watches: readonly MailWatch[];
  readonly rows: readonly MailRow[];
  readonly accounts: readonly string[];
  /** The last page came back short, so asking again returns nothing. */
  readonly ended: boolean;
}

interface Arg {
  readonly query: string;
  readonly account: string;
}

/** One page of rows: what the box answered. */
type Page = { readonly rows: readonly MailRow[]; readonly accounts: readonly string[] };

async function pageOf(arg: Arg, before: string): Promise<Page> {
  const payload =
    arg.query === ""
      ? await fetchInbox(arg.account, PAGE, before)
      : await mailSearch(arg.query, arg.account, PAGE, before);
  return { rows: payload.messages, accounts: payload.accounts };
}

function appended(listing: Listing, next: Page): Listing {
  return { ...listing, rows: [...listing.rows, ...next.rows], accounts: next.accounts, ended: next.rows.length < PAGE };
}

function countLine(arg: Arg, rows: readonly MailRow[]): HTMLElement {
  const what = arg.query === "" ? "in the inbox" : "matching";
  return el(
    "p",
    { class: "count" },
    el("strong", null, String(rows.length)),
    rows.length === 1 ? ` message ${what}` : ` messages ${what}`,
    el("span", { class: "countnote" }, "newest first; the badge is what triage made of it"),
  );
}

function searchBox(arg: Arg): HTMLElement {
  const search = el("input", {
    type: "search",
    class: "search",
    placeholder: "notmuch query — from:, subject:, date:",
    "aria-label": "search the mail with a notmuch query",
    enterkeyhint: "search",
  });
  search.value = arg.query;
  // A search is a navigation: the hash carries the query, so the result is a link and Back undoes it.
  search.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key === "Enter") address.go({ type: "mail", query: search.value.trim(), account: arg.account });
  });
  return el("div", { class: "searchbox" }, search);
}

function moreButton(arg: Arg, listing: Listing, dispatch: (next: Page) => void): HTMLElement {
  const more = el("button", { type: "button", class: "morebtn" }, "Load more", chevronIcon());
  const foot = el("div", { class: "mailfoot", hidden: listing.rows.length === 0 || listing.ended }, more);
  more.addEventListener("click", () => {
    more.setAttribute("disabled", "");
    void pageOf(arg, cursorOf(listing.rows))
      .then(dispatch)
      .catch((error: unknown) => {
        more.removeAttribute("disabled");
        foot.appendChild(el("p", { class: "problem" }, String(error)));
      });
  });
  return foot;
}

/** The first page of a listing, asked by its query and account; the pages after it are the page's own. */
const firstPages = asked.family((arg: Arg) => JSON.stringify([arg.query, arg.account]), (arg: Arg) => pageOf(arg, ""));

export function mount(root: HTMLElement, arg: Arg): Promise<Mounted> {
  return frame.mount<Listing, Page>(root, {
    className: "mailpage",
    title: arg.query === "" ? "Mail" : "Mail search",
    now: () => [
      el(
        "p",
        { class: "legend" },
        arg.query === "" ? "The inbox by date, across every account — whatever triage made of it." : `notmuch query: ${arg.query}`,
      ),
      searchBox(arg),
    ],
    reads: () =>
      reading.map(reading.both(firstPages(arg), watches.listed), ([p, ws]) =>
        appended({ watches: ws, rows: [], accounts: [], ended: false }, p),
      ),
    shape: placeholder.ROWS,
    step: appended,
    view: (listing, dispatch, again) => [
      watches.view(listing.watches, again),
      el("div", { class: "facets" }, accountChips(listing.accounts, arg.account, arg.query)),
      countLine(arg, listing.rows),
      el(
        "div",
        { class: "maillist" },
        listing.rows.length === 0
          ? empty.note(arg.query === "" ? "Nothing in the inbox for this account." : "No message in the local corpus matches that query.")
          : listing.rows.map(mailRow),
      ),
      moreButton(arg, listing, dispatch),
    ],
  });
}
