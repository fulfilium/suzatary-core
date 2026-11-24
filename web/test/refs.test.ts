/**
 * The reference grammar, in the browser's half of it. `suzatary_base::reference` is the authority;
 * `web/src/chat/refs.ts` is a second reader, because the linking happens here over prose with no offsets,
 * so where the two could disagree is pinned:
 *
 * - THE EIGHT SPELLINGS are the same eight `suzatary_base::reference`'s own test asserts, character for
 *   character: if either side changes one, one of the two files goes red.
 * - THE ROUND TRIP: whatever `print` writes, `parse` reads back as the same value.
 * - THE TOKENISER LOSES NOTHING: concatenating every token's source is the input.
 *
 * `anchor` and `textWithReferences` build nodes and belong to the browser scenes (`web/e2e/secretary.test.ts`).
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { Reference } from "../src/chat/refs";
import { hrefOf, parse, print, tokenise, urlTokens } from "../src/chat/refs";

/** The seven `SECRETARY.md` shows the model, and a triage batch's, which held decisions still
 * name, as `suzatary_base::reference::tests` asserts them. */
const SPELLINGS = [
  "todo:life/book-flight",
  "mail:a1b2c3d4e5f6",
  "thread:0000000000001b2c",
  "briefing:2026-09-10",
  "proposal:2026-09-09-1/1",
  "batch:life/2fdf1691",
  "round:default/12",
  "note:default/14",
];

test("the eight text forms are the ones the manual names, and they round-trip", () => {
  for (const spelling of SPELLINGS) {
    const reference = parse(spelling);
    assert.notEqual(reference, null, spelling);
    assert.equal(print(reference as Reference), spelling);
  }
});

test("a string that is not a reference is not one", () => {
  for (const bad of [
    "",
    "todo",
    "todo:",
    "todo:life",
    "todo:life/",
    "todo:/x",
    "todo:../x",
    "todo:life/../etc",
    "todo:Life/x",
    "mail:",
    "mail:abc",
    "mail:ABCDEF",
    "mail:zzzzzz",
    "thread:",
    "thread:a/b",
    "thread:a b",
    "briefing:",
    "briefing:2026-9-10",
    "briefing:2026-13-01",
    "briefing:tomorrow",
    "proposal:2026-09-09-1",
    "proposal:2026-09-09-1/x",
    "proposal:../etc/0",
    "batch:",
    "batch:life",
    "batch:life/",
    "batch:life/abcdef1",
    "batch:Life/abcdef1234",
    "batch:../etc/abcdef1234",
    "note:hello",
    "note:default/",
    "round:default/07",
    "round:Default/1",
    "round:../etc/1",
    "https://example.test",
  ]) {
    assert.equal(parse(bad), null, `${JSON.stringify(bad)} must not parse`);
  }
});

/** NOTHING IS DROPPED AND NOTHING IS INVENTED. */
test("the tokens reassemble into exactly the input", () => {
  for (const text of [
    "",
    "no references here at all",
    "todo:life/x",
    "todo:life/x todo:life/y",
    "(mail:a1b2c3d4e5f6) and briefing:2026-09-10.",
    "notodo:life/x is a word, xmail:a1b2c3d4e5f6 is another",
    "thread:00ab. ends a sentence",
    SPELLINGS.join(" · "),
  ]) {
    const rebuilt = tokenise(text)
      .map((token) => ("text" in token ? token.text : print(token.reference)))
      .join("");
    assert.equal(rebuilt, text, JSON.stringify(text));
  }
});

/** A reference starts at a boundary, so a word that ENDS in one is a word ("notodo:life/x"). A thread id
 * may hold a dot, so the scan refuses one at the end rather than swallowing the full stop. A bare
 * `todo:<id>` is DELIBERATELY not matched: it is typst's `#link("todo:x")` spelling, which `rich.ts` rewrites,
 * and it names a todo on whichever chain is open. */
test("a sentence splits into prose and the things it names, at boundaries only", () => {
  const cases: [string, ReturnType<typeof tokenise>][] = [
    [
      "Look at todo:life/permitfiling and mail:a1b2c3d4e5f6, then reply.",
      [
        { text: "Look at " },
        { reference: { kind: "todo", chain: "life", id: "permitfiling" } },
        { text: " and " },
        { reference: { kind: "mail", hash: "a1b2c3d4e5f6" } },
        { text: ", then reply." },
      ],
    ],
    ["notodo:life/x and xmail:a1b2c3d4e5f6", [{ text: "notodo:life/x and xmail:a1b2c3d4e5f6" }]],
    ["see thread:00ab.", [{ text: "see " }, { reference: { kind: "thread", id: "00ab" } }, { text: "." }]],
    ["todo:permitfiling", [{ text: "todo:permitfiling" }]],
    [
      "waits on todo:+3f2a9c1b2d4e/fold-law.",
      [
        { text: "waits on " },
        { reference: { kind: "global", genesis: "3f2a9c1b2d4e", id: "fold-law" } },
        { text: "." },
      ],
    ],
    ["todo:+3f2a9c1/x is too short", [{ text: "todo:+3f2a9c1/x is too short" }]],
  ];
  for (const [text, tokens] of cases) assert.deepEqual(tokenise(text), tokens, text);
});

/**
 * `urlTokens` is the decision `textWithReferences` links on (decision `boxturnslink`): an act turn's URL
 * becomes a link and a model turn's stays plain text; the gate on `who` and the anchor's `rel="noopener"`
 * are the browser scene's to prove (`web/e2e`).
 */
test("an http or https url is its own token, split out of the prose, and a non-web scheme never is", () => {
  const cases: [string, ReturnType<typeof urlTokens>][] = [
    [
      "Sign in at https://microsoft.com/devicelogin and enter the code.",
      [{ text: "Sign in at " }, { url: "https://microsoft.com/devicelogin" }, { text: " and enter the code." }],
    ],
    ["try http://example.test/x", [{ text: "try " }, { url: "http://example.test/x" }]],
    ["run javascript:alert(1) now", [{ text: "run javascript:alert(1) now" }]],
    ["open data:text/html,<script>1</script> now", [{ text: "open data:text/html,<script>1</script> now" }]],
    ["nothing to link here", [{ text: "nothing to link here" }]],
  ];
  for (const [text, tokens] of cases) assert.deepEqual(urlTokens(text), tokens, text);
});

test("urlTokens reassembles into exactly the input", () => {
  for (const text of [
    "",
    "no urls here",
    "https://a.test",
    "before https://a.test after",
    "two https://a.test and https://b.test/path?q=1",
  ]) {
    const rebuilt = urlTokens(text)
      .map((token) => ("text" in token ? token.text : token.url))
      .join("");
    assert.equal(rebuilt, text, JSON.stringify(text));
  }
});

test("a todo named by its genesis links nowhere, since the tab holds no store's genesis", () => {
  const global = parse("todo:+3f2a9c1b2d4e/fold-law");
  assert.deepEqual(global, { kind: "global", genesis: "3f2a9c1b2d4e", id: "fold-law" });
  assert.equal(hrefOf(global as Reference), null);
  assert.notEqual(hrefOf({ kind: "todo", chain: "life", id: "fold-law" }), null);
});
