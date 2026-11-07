/**
 * The one door from a string to markup; `web-static.sh` refuses any other markup-parsing assignment. It parses only
 * typst's HTML: the agent's text arrives escaped or as source, typst escapes every text it emits, and the box's
 * tripwire (`suzatary_core::tripwire`, run here through the core) refuses a script, a handler or an executable URL
 * before anything is parsed. A secretary's answer refuses `html` and `eval` instead of
 * escaping. Never hand it a string a page assembled.
 */

import { el } from "./view/dom";
import href from "./view/href";
import { hrefOf } from "./shell/route";

/** `#link("todo:<id>")` becomes the app's own route; an outbound anchor keeps only a scheme that navigates, reads as the
 * host it leads to with its label beside it, opens away from the app and cannot reach back through `window.opener`; any
 * other loses its href. Done on nodes, never the string. */
function harden(root: DocumentFragment | HTMLElement): void {
  for (const anchor of root.querySelectorAll("a[href]")) {
    href.fold(href.of(anchor.getAttribute("href") ?? "", location.href), {
      Todo: (id) => {
        anchor.setAttribute("href", hrefOf({ type: "todo", id }));
        anchor.classList.add("todolink");
      },
      Outbound: (out) => {
        const label = href.beside(out, anchor.textContent ?? "");
        anchor.textContent = out.shown;
        if (label !== "") anchor.after(document.createTextNode(` — ${label}`));
        anchor.setAttribute("rel", "noopener noreferrer");
        anchor.setAttribute("target", "_blank");
      },
      Inert: () => anchor.removeAttribute("href"),
    });
  }
}

/** typst's HTML to nodes, through an inert `<template>`: nothing is fetched or run while it parses. */
function parse(html: string): DocumentFragment {
  const template = document.createElement("template");
  template.innerHTML = html;
  harden(template.content);
  return template.content;
}

/** The box's tripwire, handed in from the core: one implementation, never a copy here. */
export type Tripwire = (html: string) => string | undefined;

/** A page the tab compiled: its body's elements, or the tripwire's reason. Refused whole, never edited. */
export type Checked =
  | { readonly type: "Elements"; readonly elements: Element[] }
  | { readonly type: "Refused"; readonly reason: string };

/** From the first `<body>` to the last `</body>`, the box's `body_of`; a fragment is its own body. */
function bodyOf(document: string): string {
  const open = document.indexOf("<body>");
  const close = document.lastIndexOf("</body>");
  return open === -1 || close < open ? document : document.slice(open + "<body>".length, close);
}

/** typst's whole HTML document, compiled in this tab, through the tripwire and then the door. */
export function checked(document: string, tripwire: Tripwire): Checked {
  const reason = tripwire(document);
  return reason === undefined ? { type: "Elements", elements: [...parse(bodyOf(document)).children] } : { type: "Refused", reason };
}

/** A page typst rendered whole, as its top-level elements, so the caller can place each one. */
export function elements(html: string): Element[] {
  return [...parse(html).children];
}

/** Rendered prose, or the stored source as text when the server sent no rendering; one function, so the fallback cannot be forgotten. */
export function richOrText(rich: string | null, source: string, attrs: Record<string, string> = {}): HTMLElement {
  const box = el("div", { ...attrs, class: `${attrs["class"] ?? ""} ${rich === null ? "assource" : "rich"}`.trim() });
  if (rich === null) box.textContent = source;
  else box.appendChild(parse(rich));
  return box;
}

/** The same, inline, so a card's clamped body keeps the line-clamp's box model. */
export function richOrTextInline(rich: string | null, source: string, className: string): HTMLElement {
  const box = el("span", { class: `${className} ${rich === null ? "assource" : "rich"}` });
  if (rich === null) box.textContent = source;
  else box.appendChild(parse(rich));
  return box;
}
