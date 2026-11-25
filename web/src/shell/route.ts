// The address: the hash parsed once into a closed sum, total over every string, and printed back by `hrefOf`.

export type Route =
  | { readonly type: "todos" }
  // What a share target opens: the list with the add sheet up. In the fragment, so the box never sees the text.
  | { readonly type: "add"; readonly text: string; readonly title: string }
  | { readonly type: "todo"; readonly id: string }
  // The open list drawn by a Typst view compiled in the tab, the todo it has open ("" for none).
  | { readonly type: "typst"; readonly open: string }
  // The inbox, with the proposal a push named open.
  | { readonly type: "inbox"; readonly open?: string }
  // A notmuch query ("" is the inbox) and an account ("" is all): both in the hash, so a search is a link.
  | { readonly type: "mail"; readonly query: string; readonly account: string }
  // By notmuch id or a todo's `source.hash`.
  | { readonly type: "message"; readonly id: string }
  | { readonly type: "thread"; readonly id: string }
  | { readonly type: "about" }
  | { readonly type: "settings" }
  | { readonly type: "briefings" }
  | { readonly type: "briefing"; readonly date: string }
  // A week of the calendars from a day, "" for today.
  | { readonly type: "calendar"; readonly date: string }
  // The stream filtered by `topics`; with references to attach or the compose box to focus ("Ask the secretary"), it
  // forwards to the default conversation.
  | { readonly type: "secretary"; readonly refs: readonly string[]; readonly topics: readonly string[]; readonly focus?: boolean }
  | { readonly type: "memory" }
  // A topic not yet asked anything: the box holds nothing of it until its first message is sent.
  | { readonly type: "newtopic" }
  | { readonly type: "conversations" }
  // `at` is a search hit's turn: the window opens ending at it.
  | { readonly type: "conversation"; readonly id: string; readonly refs: readonly string[]; readonly focus?: boolean; readonly at?: number }
  // The secretary beside a page: the default conversation's talk about a reference, "" for the whole conversation.
  | { readonly type: "aside"; readonly about: string }
  | { readonly type: "unknown"; readonly hash: string };

export const HOME: Route = { type: "todos" };

function decode(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}

/** A hash's query half; a value that will not decode reads as absent. */
function params(search: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const pair of search.split("&")) {
    if (pair === "") continue;
    const cut = pair.indexOf("=");
    const name = decode(cut === -1 ? pair : pair.slice(0, cut));
    const value = decode(cut === -1 ? "" : pair.slice(cut + 1));
    if (name !== null && name !== "" && value !== null) out[name] = value;
  }
  return out;
}

function search(fields: Record<string, string>): string {
  const pairs = Object.entries(fields)
    .filter(([, value]) => value !== "")
    .map(([name, value]) => `${name}=${encodeURIComponent(value)}`);
  return pairs.length === 0 ? "" : `?${pairs.join("&")}`;
}

/** A comma-separated field, empty entries dropped, so `ref=` and no `ref` are one thing. */
function list(value: string): readonly string[] {
  return value.split(",").filter((one) => one !== "");
}

export function parseRoute(hash: string): Route {
  const trimmed = hash.startsWith("#") ? hash.slice(1) : hash;
  // An unknown address names itself as a hash, with or without its `#` written (as `Route::parse`).
  const unknown: Route = { type: "unknown", hash: `#${trimmed}` };
  const whole = trimmed.startsWith("/") ? trimmed.slice(1) : trimmed;
  const mark = whole.indexOf("?");
  const path = mark === -1 ? whole : whole.slice(0, mark);
  const query = params(mark === -1 ? "" : whole.slice(mark + 1));
  const cut = path.indexOf("/");
  const head = cut === -1 ? path : path.slice(0, cut);
  const rest = cut === -1 ? "" : path.slice(cut + 1);
  switch (head) {
    case "":
    case "todos":
      return HOME;
    case "add":
      // A bare `#/add` is the list: what opens the sheet is having something to put in it.
      return { type: "add", text: query["text"] ?? "", title: query["title"] ?? "" };
    case "inbox":
      return query["open"] === undefined || query["open"] === "" ? { type: "inbox" } : { type: "inbox", open: query["open"] };
    case "todo": {
      const id = decode(rest);
      return id === null || id === "" ? unknown : { type: "todo", id };
    }
    case "typst": {
      const open = decode(rest);
      return open === null ? unknown : { type: "typst", open };
    }
    case "mail": {
      if (rest === "") return { type: "mail", query: query["q"] ?? "", account: query["account"] ?? "" };
      // `thread/` cannot collide with an id: an id's slashes arrive as `%2F`.
      if (rest.startsWith("thread/")) {
        const id = decode(rest.slice("thread/".length));
        return id === null || id === "" ? unknown : { type: "thread", id };
      }
      const id = decode(rest);
      return id === null || id === "" ? unknown : { type: "message", id };
    }
    case "secretary": {
      const refs = list(query["ref"] ?? "");
      // Absent rather than false, so a literal with no `focus` still equals what this parses.
      const focus: { readonly focus?: true } = query["focus"] === "1" ? { focus: true } : {};
      if (rest === "") return { type: "secretary", refs, topics: list(query["topics"] ?? ""), ...focus };
      if (rest === "memory") return { type: "memory" };
      if (rest === "new") return { type: "newtopic" };
      const id = decode(rest);
      // Digits a number holds exactly, as `Route::parse` reads them; past 2^53 − 1 it is absent.
      const at = /^\d+$/.test(query["at"] ?? "") && Number.isSafeInteger(Number(query["at"])) ? { at: Number(query["at"]) } : {};
      return id === null || id === ""
        ? unknown
        : { type: "conversation", id, refs, ...focus, ...at };
    }
    case "conversations":
      return rest === "" ? { type: "conversations" } : unknown;
    case "aside": {
      const about = decode(rest);
      return about === null ? unknown : { type: "aside", about };
    }
    case "about":
      return rest === "" ? { type: "about" } : unknown;
    case "settings":
      return rest === "" ? { type: "settings" } : unknown;
    case "calendar": {
      const day = decode(rest);
      return day === null ? unknown : { type: "calendar", date: day };
    }
    case "briefings": {
      if (rest === "") return { type: "briefings" };
      const day = decode(rest);
      return day === null || day === "" ? unknown : { type: "briefing", date: day };
    }
    default:
      return unknown;
  }
}

export function hrefOf(route: Route): string {
  switch (route.type) {
    case "todos":
      return "#/";
    case "add":
      return `#/add${search({ text: route.text, title: route.title })}`;
    case "inbox":
      return `#/inbox${route.open === undefined ? "" : search({ open: route.open })}`;
    case "todo":
      return `#/todo/${encodeURIComponent(route.id)}`;
    case "typst":
      return route.open === "" ? "#/typst" : `#/typst/${encodeURIComponent(route.open)}`;
    case "mail":
      return `#/mail${search({ q: route.query, account: route.account })}`;
    case "message":
      return `#/mail/${encodeURIComponent(route.id)}`;
    case "thread":
      return `#/mail/thread/${encodeURIComponent(route.id)}`;
    case "secretary":
      return `#/secretary${search({ ref: route.refs.join(","), topics: route.topics.join(","), focus: route.focus === true ? "1" : "" })}`;
    case "memory":
      return "#/secretary/memory";
    case "newtopic":
      return "#/secretary/new";
    case "conversations":
      return "#/conversations";
    case "aside":
      return route.about === "" ? "#/aside" : `#/aside/${encodeURIComponent(route.about)}`;
    case "conversation":
      return `#/secretary/${encodeURIComponent(route.id)}${search({
        ref: route.refs.join(","),
        focus: route.focus === true ? "1" : "",
        at: route.at === undefined ? "" : String(route.at),
      })}`;
    case "about":
      return "#/about";
    case "settings":
      return "#/settings";
    case "briefings":
      return "#/briefings";
    case "briefing":
      return `#/briefings/${encodeURIComponent(route.date)}`;
    case "calendar":
      return route.date === "" ? "#/calendar" : `#/calendar/${encodeURIComponent(route.date)}`;
    case "unknown":
      return route.hash;
  }
}

// "Ask the secretary" with something to ask is the default conversation, so the place written is that one and the
// log never holds a route that only forwards.
export const forwarded = (route: Route): Route =>
  route.type === "secretary" && (route.refs.length > 0 || route.focus === true)
    ? { type: "conversation", id: "default", refs: route.refs, ...(route.focus === true ? { focus: true } : {}) }
    : route

// `tree` is the desk's print after `~` (`viewtree::tiles::print`), "" where the bare route says the tiles; the route is the focused tile's.
export type Address = { readonly route: Route; readonly tree: string };

export const bare = (route: Route): Address => ({ route, tree: "" });

export const hrefOfAddress = (a: Address): string => (a.tree === "" ? hrefOf(a.route) : `${hrefOf(a.route)}~${a.tree}`);

// A tree's print starts with its axis, `|` or `-`. A hash whose text after its last `~` does not is a route alone; one
// whose text does is a tree, damaged or not, which the desk reads (a damaged one as its head's page) and prints back.
const NODE = /^[|-]/;

export function parseAddress(hash: string): Address {
  const cut = hash.lastIndexOf("~");
  const tree = cut === -1 ? "" : hash.slice(cut + 1);
  return cut !== -1 && NODE.test(tree) ? { route: parseRoute(hash.slice(0, cut)), tree } : bare(parseRoute(hash));
}

export const forwardedAddress = (a: Address): Address => {
  const route = forwarded(a.route);
  return route === a.route ? a : { route, tree: a.tree };
};
