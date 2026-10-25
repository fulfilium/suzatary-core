/**
 * References in the page: `print` writes the text form, `parse` reads one, `tokenise` finds them in prose.
 * A tokeniser, never a markup parser: a turn's text is the least trusted string served, so it becomes
 * text nodes and built anchors only, never `rich.ts`.
 * The Rust is the authority on the grammar; this second reader exists because prose arrives with no
 * offsets, and `web/test/refs.test.ts` pins it to the Rust's spellings.
 * A bare `todo:<id>` is deliberately not matched: it names a todo on whichever chain is open.
 */

import type { TurnRow } from "../wire/generated/wire";
import { el } from "../view/dom";
import { hrefOf as routeHref } from "../shell/route";

/** `reference::Reference` in parts. The wire and the store carry its text form; this is what `parse` reads it into. */
export type Reference =
  | { readonly kind: "todo"; readonly chain: string; readonly id: string }
  | { readonly kind: "mail"; readonly hash: string }
  | { readonly kind: "thread"; readonly id: string }
  | { readonly kind: "briefing"; readonly date: string }
  | { readonly kind: "proposal"; readonly conversation: string; readonly index: number }
  | { readonly kind: "batch"; readonly chain: string; readonly head: string }
  | { readonly kind: "round" | "note"; readonly conversation: string; readonly turn: number };

/**
 * Group 1 is the word boundary, captured because the bundle's target cannot promise lookbehind.
 * Group 2 uses the Rust's character sets, narrowed so a thread id cannot end on a sentence's dot.
 */
const PATTERN = new RegExp(
  "(^|[^A-Za-z0-9_-])(" +
    "todo:[a-z][a-z0-9-]*\\/[a-z0-9_-]+" +
    "|mail:[0-9a-f]{6,64}" +
    "|thread:[A-Za-z0-9_-](?:[A-Za-z0-9._-]{0,126}[A-Za-z0-9_-])?" +
    "|briefing:\\d{4}-\\d{2}-\\d{2}" +
    "|proposal:[a-z0-9][a-z0-9-]*\\/\\d+" +
    "|batch:[a-z][a-z0-9-]*\\/[0-9a-f]{8,64}" +
    "|(?:round|note):[a-z0-9][a-z0-9-]*\\/[0-9]+" +
    ")",
  "g",
);

/** The text form the model writes and the store holds; the inverse of `parse`. */
export function print(reference: Reference): string {
  switch (reference.kind) {
    case "todo":
      return `todo:${reference.chain}/${reference.id}`;
    case "mail":
      return `mail:${reference.hash}`;
    case "thread":
      return `thread:${reference.id}`;
    case "briefing":
      return `briefing:${reference.date}`;
    case "proposal":
      return `proposal:${reference.conversation}/${reference.index}`;
    case "batch":
      return `batch:${reference.chain}/${reference.head}`;
    case "round":
    case "note":
      return `${reference.kind}:${reference.conversation}/${reference.turn}`;
  }
}

/** `null` for anything the Rust would refuse: erring toward "not a link" is the safe direction. */
export function parse(text: string): Reference | null {
  const cut = text.indexOf(":");
  if (cut === -1) return null;
  const rest = text.slice(cut + 1);
  switch (text.slice(0, cut)) {
    case "todo": {
      const slash = rest.indexOf("/");
      if (slash <= 0 || slash === rest.length - 1) return null;
      const chain = rest.slice(0, slash);
      const id = rest.slice(slash + 1);
      if (!/^[a-z][a-z0-9-]{0,31}$/.test(chain)) return null;
      if (!/^[a-z0-9_-]+$/.test(id)) return null;
      return { kind: "todo", chain, id };
    }
    case "mail":
      return /^[0-9a-f]{6,64}$/.test(rest) ? { kind: "mail", hash: rest } : null;
    case "thread":
      return /^[A-Za-z0-9._-]{1,128}$/.test(rest) ? { kind: "thread", id: rest } : null;
    case "briefing":
      return isDay(rest) ? { kind: "briefing", date: rest } : null;
    case "proposal": {
      const slash = rest.indexOf("/");
      if (slash <= 0) return null;
      const conversation = rest.slice(0, slash);
      const index = rest.slice(slash + 1);
      if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(conversation)) return null;
      if (!/^\d{1,9}$/.test(index)) return null;
      return { kind: "proposal", conversation, index: Number(index) };
    }
    case "batch": {
      const slash = rest.indexOf("/");
      if (slash <= 0) return null;
      const chain = rest.slice(0, slash);
      const head = rest.slice(slash + 1);
      if (!/^[a-z][a-z0-9-]{0,31}$/.test(chain)) return null;
      if (!/^[0-9a-f]{8,64}$/.test(head)) return null;
      return { kind: "batch", chain, head };
    }
    case "round":
      return target("round", rest);
    case "note":
      return target("note", rest);
    default:
      return null;
  }
}

/** `topics::target::Target`'s grammar: a conversation id and a turn with no leading zero. */
function target(kind: "round" | "note", rest: string): Reference | null {
  const slash = rest.lastIndexOf("/");
  if (slash <= 0) return null;
  const conversation = rest.slice(0, slash);
  const turn = rest.slice(slash + 1);
  if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(conversation) || conversation === "memory") return null;
  if (!/^(0|[1-9]\d{0,15})$/.test(turn)) return null;
  return { kind, conversation, turn: Number(turn) };
}

/** Cheaper than the Rust's chrono check; a 31 February only links to a briefing that says it does not exist. */
function isDay(text: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
  if (match === null) return false;
  const month = Number(match[2]);
  const day = Number(match[3]);
  return month >= 1 && month <= 12 && day >= 1 && day <= 31;
}

/** A todo's route drops the chain, the route's own decision; a proposal, a round and a note go to their conversation, a batch to its card. */
export function hrefOf(reference: Reference): string {
  switch (reference.kind) {
    case "todo":
      return routeHref({ type: "todo", id: reference.id });
    case "mail":
      return routeHref({ type: "message", id: reference.hash });
    case "thread":
      return routeHref({ type: "thread", id: reference.id });
    case "briefing":
      return routeHref({ type: "briefing", date: reference.date });
    case "proposal":
      return routeHref({ type: "conversation", id: reference.conversation, refs: [] });
    case "batch":
      return routeHref({ type: "inbox" });
    case "round":
    case "note":
      return routeHref({ type: "conversation", id: reference.conversation, refs: [] });
  }
}

/** Labelled with the text form, so a reader learns the spelling by seeing it; a card names it in words instead. */
export function anchor(reference: Reference, className = "reflink", label = print(reference)): HTMLElement {
  return el("a", { class: `${className} ref-${reference.kind}`, href: hrefOf(reference) }, label);
}

/** A reference off the wire, as a link; text the grammar refuses stays text. */
export const anchorOf = (text: string, className = "reflink"): HTMLElement | string => {
  const reference = parse(text);
  return reference === null ? text : anchor(reference, className);
};

/** The references among these texts, in order. */
export const parsed = (texts: readonly string[]): Reference[] =>
  texts.flatMap((text) => {
    const reference = parse(text);
    return reference === null ? [] : [reference];
  });

/** A link, not a button: it is a navigation, so it middle-clicks, can be sent, and Back returns here. */
export function askHref(reference: Reference): string {
  return routeHref({ type: "secretary", refs: [print(reference)], topics: [] });
}

export function askLink(reference: Reference): HTMLElement {
  return el("a", { class: "asksecretary", href: askHref(reference) }, "Ask the secretary");
}

export type Token = { readonly text: string } | { readonly reference: Reference };

export type UrlToken = { readonly text: string } | { readonly url: string };

/** Total: the tokens concatenate back to the input, so the page never misquotes the model. */
export function tokenise(text: string): Token[] {
  const out: Token[] = [];
  let at = 0;
  PATTERN.lastIndex = 0;
  for (let match = PATTERN.exec(text); match !== null; match = PATTERN.exec(text)) {
    const spelling = match[2] ?? "";
    const reference = parse(spelling);
    if (reference === null) continue;
    const start = match.index + (match[1] ?? "").length;
    if (start > at) out.push({ text: text.slice(at, start) });
    out.push({ reference });
    at = start + spelling.length;
  }
  if (at < text.length) out.push({ text: text.slice(at) });
  return out;
}

/**
 * URLs are linked only in an act turn, the box's own words (the sign-in URL); a model's URL stays text,
 * since a crafted mail could aim a tap with it. The scheme is checked by `URL`, not trusted from the match.
 */
const URL_PATTERN = /https?:\/\/[^\s<>"']+/g;

function schemeIsLinkable(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function urlTokens(text: string): UrlToken[] {
  const out: UrlToken[] = [];
  let at = 0;
  URL_PATTERN.lastIndex = 0;
  for (let match = URL_PATTERN.exec(text); match !== null; match = URL_PATTERN.exec(text)) {
    const url = match[0];
    if (!schemeIsLinkable(url)) continue;
    const start = match.index;
    if (start > at) out.push({ text: text.slice(at, start) });
    out.push({ url });
    at = start + url.length;
  }
  if (at < text.length) out.push({ text: text.slice(at) });
  return out;
}

// `noopener`: the opened tab gets no `window.opener` back into this one.
function urlAnchor(url: string): HTMLElement {
  return el("a", { class: "actlink", href: url, target: "_blank", rel: "noopener" }, url);
}

function proseWithUrls(text: string, linkify: boolean): Node[] {
  if (!linkify) return [document.createTextNode(text)];
  return urlTokens(text).map((token) =>
    "url" in token ? urlAnchor(token.url) : document.createTextNode(token.text),
  );
}

/** `who` is required on purpose, so every call site names whose words these are before a URL is linked. */
export function textWithReferences(text: string, who: TurnRow["who"]): Node[] {
  return tokenise(text).flatMap((token) =>
    "text" in token ? proseWithUrls(token.text, who === "act" || who === "box") : [anchor(token.reference)],
  );
}
