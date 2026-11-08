/**
 * THE BROWSER'S CORE, AGAINST THE REFERENCE'S OWN VECTORS.
 *
 * The Prodrome's own suite already checks the Rust against its seeded vectors.
 * This checks something those tests cannot: that the WebAssembly BUILD of it,
 * loaded and called across the JS boundary exactly as a tab does, still
 * produces the reference's numbers. Every step between — `wasm-bindgen`'s glue,
 * `wasm-opt`'s rewrite, the JSON shapes in `web/src/prodrome/core.ts`, its guards — is a
 * place an answer could change, and none of them is covered by a `cargo test`.
 *
 * THE VECTORS COME FROM TWO PLACES, and which place a file is in says what it is evidence ABOUT.
 * `$SUZATARY_PRODROME_CONFORMANCE` is the seeded, frozen evidence that
 * travels with the crate — since prodrome 0.4 (`conformance/*.py`, "the
 * evidence is a literal") it is the crate's own §2 grammar rather than JSON,
 * so this points at `app/core/examples/vector_json.rs`'s output, which reads
 * `fpl.py`/`series.py` with the crate's own parser (never a second one —
 * see `suzatary-vector-json`'s comment in `flake.nix`) and re-emits the SAME 900
 * samples and 1429 knots as the `fpl.json`/`series.json` this file always
 * read, so nothing below this comment changed. The live chain's objects are
 * read from the data repository (`$SUZATARY_LIVE_EVENTS`); this repository
 * holds no print of the chain's contents.
 *
 * SO THE CLAIM IS THE SPEC'S §9.8, MEASURED HERE: hashes and knot instants
 * EXACTLY, fulfillments to 1e-9. Not "the module loads".
 *
 * The module is the `--target nodejs` glue from the same derivation and the
 * same `.wasm` the browser gets (`$SUZATARY_PRODROME_WASM`, set by
 * scripts/web-test.sh). node 24 needs no flag for it; `createRequire` is the one
 * door, because that glue is CommonJS and reads its `.wasm` off disk.
 *
 * The functions under test take a `Core` rather than importing the module, so
 * the code exercised here is the code the tab runs — the guards included.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";

import type { Core, CoreEnv, Todos } from "../src/prodrome/core";
import { found } from "../src/prodrome/verify";
import signer from "../src/prodrome/signer";
import {
  linked,
  linkedKnots,
  appendTo,
  keyAdded,
  lifecycleEvent,
  provenanceOf,
  signedObject,
  standingOf,
  objectsOf,
  termJson,
  verifyObjects,
} from "../src/prodrome/core";

/** The built core, from the flake's derivation. A missing variable is a
 * FAILURE and never a skip: a suite that quietly stops checking the thing it
 * is named after is worse than one that is red. */
function loaded(): Core {
  const built = process.env["SUZATARY_PRODROME_WASM"];
  if (built === undefined || built === "") {
    throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh");
  }
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core;
}

const core = loaded();

/** The SEEDED vectors, from the Prodrome's `conformance/`. A missing
 * variable is a FAILURE and never a skip, for the reason `loaded()` gives. */
function vectors<T>(name: string): T {
  const dir = process.env["SUZATARY_PRODROME_CONFORMANCE"];
  if (dir === undefined || dir === "") {
    throw new Error(
      "SUZATARY_PRODROME_CONFORMANCE is unset — run scripts/web-test.sh",
    );
  }
  return JSON.parse(readFileSync(`${dir}/${name}.json`, "utf8")) as T;
}

// --- §2/§3: the hashes, exactly ---------------------------------------------

/** The live store's objects, as `/api/chain` would hand them over — read off
 * `events/objects/` rather than out of a copy of it, which is what makes this a
 * claim about the bytes on this disk. The whole store, not a sample: rehashing
 * is what the chain page's verdict rests on and hundreds of sha256s cost
 * milliseconds. It GROWS, so nothing here counts them. */
const store = process.env["SUZATARY_LIVE_EVENTS"];
const objects = (store === undefined ? [] : readdirSync(`${store}/objects`))
  .sort()
  .map((file) => ({
    hash: file.replace(/\.py$/, ""),
    literal: readFileSync(`${store}/objects/${file}`, "utf8"),
  }));
/** The data tree is its own repository (datasplit): a clone of the code alone
 * has no chain, and these tests say so rather than pass over nothing. */
const onBox: { skip: string | false } = { skip: store === undefined ? "no data tree: SUZATARY_LIVE_EVENTS is unset" : false };

test("every stored object hashes to the name it is filed under", onBox, () => {
  // The core derives the tips from the objects themselves (prodrome 0.9):
  // nobody tells it where the heads are.
  const verdict = verifyObjects(core, objects);

  assert.equal(verdict.objects.length, objects.length);
  for (const row of verdict.objects) {
    assert.equal(row.computed, row.hash, `object ${row.hash} rehashed to ${row.computed}`);
    assert.equal(row.hashOk, true);
  }
  assert.equal(verdict.verified, objects.length);
  // And the core PARSED every one of them, which the SubtleCrypto path cannot
  // do: a row with a problem here is a grammar failure, not a hash failure.
  assert.deepEqual(
    verdict.objects.filter((row) => row.condition.type !== "Clean").map((row) => row.hash),
    [],
  );
  assert.equal(found.problems(verdict.found).length, 0, found.problems(verdict.found).join(" | "));
  assert.equal(verdict.found.type, "Reached");
  // A causal order over the whole set, every parent before its child.
  assert.equal(verdict.linearisation.length, objects.length);
  const placed = new Set<string>();
  for (const name of verdict.linearisation) {
    const object = objects.find((o) => o.hash === name);
    const prev = /^Sealed\(prev='([0-9a-f]{64})'/.exec(object?.literal ?? "");
    if (prev !== null) assert.equal(placed.has(prev[1] ?? ""), true, `${name} came before its parent`);
    placed.add(name);
  }
});

test("a tampered literal is caught by the core, on its own object", onBox, () => {
  const genesis = objects[0];
  if (genesis === undefined) throw new Error("events/objects/ holds no objects");
  const tampered = [{ hash: genesis.hash, literal: `${genesis.literal} ` }];
  const verdict = verifyObjects(core, tampered);
  assert.equal(verdict.found.type, "Unreached");
  assert.equal(verdict.verified, 0);
  assert.match(found.problems(verdict.found).join(" | "), /does not hash to its own name/);
});

test("an object that hashes but is not the grammar is caught too", () => {
  // The one thing the SubtleCrypto fallback structurally cannot check. The
  // name is the real sha256 of this text, so only a PARSER can refuse it.
  const literal = "Sealed(prev='', event=Invented(todo='x'))";
  const hash = "0af7ffadfce0140c4d2d5a70e2370968a3521fe6d5cb2e5a8d5f6f4c0bb463e9";
  const verdict = verifyObjects(core, [{ hash, literal }]);
  assert.equal(verdict.objects[0]?.hashOk, true, "the fixture must hash to its own name");
  assert.equal(verdict.found.type, "Unreached");
  assert.match(found.problems(verdict.found).join(" | "), /failed to parse/);
});

// --- §7: the fulfillments, to 1e-9 ------------------------------------------

interface FplVectors {
  terms: {
    // The frozen vectors predate 0.8.0: their environment is the outcomes half.
    env: CoreEnv["outcomes"];
    json: unknown;
    samples: { now: string; value: number }[];
    term: string;
  }[];
}

const fpl = vectors<FplVectors>("fpl").terms;

test("every fulfillment sample agrees with the reference to 1e-9", () => {
  let checked = 0;
  let worst = 0;
  for (const vector of fpl) {
    // Through `term_json` from the PRINT, not from the vector's `json`: that
    // exercises §2's parser and §7's JSON printer in the same breath, and the
    // print is the form the chain actually stores.
    const term = termJson(core, vector.term);
    assert.deepEqual(term, vector.json, `to_json(parse(${vector.term})) drifted`);
    for (const sample of vector.samples) {
      const mine = linked(core, JSON.stringify, { outcomes: vector.env, tended: {} }, sample.now)(term);
      if (mine === null) throw new Error(`${vector.term} at ${sample.now} read ∅, the vector says ${sample.value}`);
      const off = Math.abs(mine - sample.value);
      worst = Math.max(worst, off);
      assert.ok(off <= 1e-9, `${vector.term} at ${sample.now}: ${mine} vs ${sample.value}`);
      checked += 1;
    }
  }
  assert.equal(checked, 900, "the vector file's 900 samples, all of them");
  assert.ok(worst < 1e-9, `largest deviation ${worst}`);
});

// --- §7 knots: the curves, instant for instant -------------------------------

interface SeriesVectors {
  series: { exact: boolean; from: string; knots: [string, number][]; term: string; to: string }[];
}

const series = vectors<SeriesVectors>("series").series;

test("every series has the reference's knots, at the reference's instants", () => {
  let knots = 0;
  for (const vector of series) {
    // No `history`: these vectors have no environment, and an empty one is the
    // honest argument rather than a stand-in for one.
    const mine = linkedKnots(core, JSON.stringify, { bindings: {}, tended: {} }, vector.from, vector.to)(termJson(core, vector.term));
    assert.equal(mine.exact, vector.exact, `${vector.term}: exactness`);
    assert.equal(mine.knots.length, vector.knots.length, `${vector.term}: knot count`);
    for (const [i, knot] of mine.knots.entries()) {
      const theirs = vector.knots[i];
      if (theirs === undefined) throw new Error(`${vector.term}: knot ${i} is missing from the vector`);
      // The INSTANTS are exact — a knot at a different moment is a different
      // curve, however close its value.
      assert.equal(knot[0], theirs[0], `${vector.term}: knot ${i} instant`);
      const value = knot[1];
      assert.ok(value !== null && Math.abs(value - theirs[1]) <= 1e-9, `${vector.term}: knot ${i} value`);
      knots += 1;
    }
  }
  assert.equal(knots, 1429, "the vector file's 1429 knots, all of them");
});

// --- the guards --------------------------------------------------------------

test("a refusal from the core arrives as a value, not as a dead module", () => {
  assert.throws(() => termJson(core, "Flat(value=2.0)"), /value/);
  assert.throws(() => linked(core, JSON.stringify, { outcomes: {}, tended: {} }, "not-an-instant")({ kind: "flat", value: 0.5 }), /now/);
  // And the module still answers afterwards: a refusal must not poison it.
  assert.equal(linked(core, JSON.stringify, { outcomes: {}, tended: {} }, "2026-09-06T12:00:00")({ kind: "flat", value: 0.5 }), 0.5);
});

// --- §3/§4: appending, which is how a replica acts ---------------------------

/** One question of a replica holding `held`, its memory given back however it ends. */
const holding = <T>(held: readonly { hash: string; literal: string }[], ask: (t: Todos) => T): T => {
  const t = new core.Todos(objectsOf(held));
  try {
    return ask(t);
  } finally {
    t.free();
  }
};

test("a lifecycle event the core spelled appends to the live chain as a change the core names", onBox, () => {
  // THE WHOLE OF WHAT A PHONE NEEDS TO WRITE, driven against the same `.wasm`
  // the tab loads and THIS box's objects. `lifecycle` goes through §4's own
  // constructor, `append` through §3's change and §2's printer, and the name is
  // the sha256 of that print — which is how a replica and a box agree on what
  // an object is CALLED with nothing between them but bytes.
  const event = lifecycleEvent(core, "Completed", "alpha", "2026-09-06T12:00:00", "ada");
  assert.match(event, /^Completed\(todo='alpha', at=datetime\(2026, 9, 6, 12, 0, 0\)/, event);

  const [object, ...more] = holding(objects, (t) => appendTo(t, event).objects);
  if (object === undefined || more.length > 0) throw new Error("one new object");
  assert.match(object.literal, /^Change\(genesis='[0-9a-f]{64}', deps=\(/);
  assert.equal(
    object.hash,
    createHash("sha256").update(object.literal, "utf8").digest("hex"),
    "the name is sha256 of the print, which is what the box rehashes on receipt",
  );
  const verdict = verifyObjects(core, [...objects, object]);
  assert.equal(verdict.found.type, "Reached", found.problems(verdict.found).join("; "));
});

test("the core refuses to append what its own constructors would refuse to build", onBox, () => {
  // An id no writer on the chain may spell.
  assert.throws(() => lifecycleEvent(core, "Completed", "Alpha", "2026-09-06T12:00:00", "ada"));
  // An instant that is not one — a browser's clock is an argument, not a fact,
  // and an unparseable one is refused rather than guessed at.
  assert.throws(() => lifecycleEvent(core, "Completed", "alpha", "yesterday", "ada"));
  // And bytes that are not an event cannot be appended: the print is read back
  // through §2's grammar before anything is wrapped in it.
  assert.throws(() => holding(objects, (t) => appendTo(t, "Nonsense()")));
});

// The tab's door refuses what the box refuses because it asks the box's own check, built into this module.
test("the module's tripwire is the box's: it names the box's reasons and passes typst's escaped text", () => {
  assert.equal(core.suspicious("<ScRiPt>alert(1)</script>"), "an element typst never writes");
  assert.equal(core.suspicious(`<img ONCLICK = "x()">`), "an attribute typst never writes");
  assert.equal(core.suspicious(`<a href = 'data:text/html,x'>go</a>`), "a URL that does not navigate");
  assert.equal(core.suspicious(`<a href="java	script:x">go</a>`), "a URL that does not navigate");
  assert.equal(core.suspicious("<p>&lt;script>alert(1)&lt;/script> #strong[x] ]</p>"), undefined);
});

// A Typst view's state holds prose, read back from escaped markup by the core's own inverse of the escape.
test("the module reads escaped markup back as its prose, and leaves a person's markup alone", () => {
  assert.equal(core.prose("\\#strong\\[x\\] \\<script\\> \\]"), "#strong[x] <script> ]");
  assert.equal(core.prose("plain words"), "plain words");
  assert.equal(core.prose("*bold* words"), undefined);
});

// --- SPEC law 42: a device's WebCrypto signature, through the core --------------------------

test("a signature this browser makes with WebCrypto is one the core seals, and a moved one is refused", async () => {
  const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
  if (!("privateKey" in pair)) throw new Error("no key pair");
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  const key = { public: Array.from(raw, (b) => b.toString(16).padStart(2, "0")).join(""), key: pair.privateKey };
  const genesis = "Genesis(label='todos', nonce='00000000000000000000000000000000')";
  const named = { hash: createHash("sha256").update(genesis, "utf8").digest("hex"), literal: genesis };
  const signature = await signer.sign(key, named.hash);
  const sealed = signedObject(core, named.hash, key.public, signature);
  assert.match(sealed.literal, new RegExp(`^Signed\\(object='${named.hash}', key='${key.public}'`));
  assert.equal(sealed.hash, createHash("sha256").update(sealed.literal, "utf8").digest("hex"), "named by its own print");
  const other = "0".repeat(64);
  assert.throws(() => signedObject(core, other, key.public, signature), /does not verify/);

  // A key no root registered is untrusted, and the registration a card would ask for has a name a device can sign.
  assert.equal(standingOf(core, [named], "1".repeat(64), "ada", key.public), "untrusted");
  assert.match(keyAdded(core, [named], "ada", key.public), /^[0-9a-f]{64}$/);
  const wrote = provenanceOf(core, [named, sealed], null);
  assert.deepEqual(wrote, [], "a genesis and a signature carry no actor's word");
});
