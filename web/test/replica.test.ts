/**
 * THE REPLICA'S DECISIONS, on their own.
 *
 * `replica.ts` is where the rules live as pure functions, so they can be tested
 * here rather than through a browser: what to hand the box, what to ask it for,
 * what a receipt teaches, and what a refusal holds back are all decisions over
 * SETS OF NAMES. The effectful half (`sync.ts`) is behaviour, and `web/e2e/` is
 * where that is checked against a real server.
 *
 * This replaced `queue.test.ts`, and the two are worth comparing: that file
 * tested a queue of WISHES — enqueue, dequeue-on-applied, mark-failed, a cap, a
 * nonce to settle by. None of those exist. An object is a FACT this replica
 * already holds, so the only questions left are which ones the box has and
 * which ones it will not take.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ReceiptPayload, StoredObject } from "../src/wire/generated/wire";
import { KNOWN, replicaWith } from "./held";
import { parseReplica } from "../src/prodrome/objects";
import lo from "../src/algebra/lo";
import {
  NOTHING,
  appended,
  boxHeadsOf,
  rootOf,
  untrustedOf,
  headsOf,
  parentsOf,
  pulled,
  sinceOf,
  awaitingTrust,
  settled,
  stuckBehind,
  toPush,
  unsignedWrites,
  unsynced,
  unsentTodos,
  wentBack,
} from "../src/prodrome/replica";

/** A name that looks like one: 64 hex digits, as every object's is. */
const name = (seed: string): string => seed.repeat(64).slice(0, 64);

const A = name("a");
const B = name("b");
const C = name("c");
const D = name("d");

/** One object's print, in the shape the store holds: an envelope naming its
 * parents, then the event. Only the ENVELOPE half matters to this file. */
const sealed = (hash: string, prev: string, todo = "alpha"): StoredObject => ({
  hash,
  literal: `Sealed(prev='${prev}', event=Completed(todo='${todo}', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', note=''))`,
});

const change = (hash: string, genesis: string, deps: readonly string[]): StoredObject => ({
  hash,
  literal: `Change(genesis='${genesis}', deps=(${deps.map((d) => `'${d}',`).join(" ")}), event=Completed(todo='alpha', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', note=''))`,
});

const woven = (hash: string, parents: readonly string[]): StoredObject => ({
  hash,
  literal: `Woven(parents=(${parents.map((p) => `'${p}'`).join(", ")}), event=None)`,
});

const replica = replicaWith;
const ROOT = name("f");

// --- Reading a print's shape --------------------------------------------------

test("a print's parents are the object names in its envelope, and nothing after", () => {
  assert.deepEqual(parentsOf(sealed(B, A)), [A]);
  assert.deepEqual(parentsOf(sealed(A, "")), [], "genesis rests on nothing");
  assert.deepEqual(parentsOf(woven(C, [A, B])), [A, B]);
});

test("a change's parents are its deps, never the genesis it names", () => {
  assert.deepEqual(parentsOf(change(C, A, [B])), [B]);
  assert.deepEqual(parentsOf(change(C, A, [])), [], "a first write to a todo rests on no object");
});

test("a genesis a change rests in is no head, as the box's tips read it", () => {
  // Not a parent (above), and not a head either: a head the box never names would leave every local fold behind it.
  assert.deepEqual(headsOf([genesisObject(A), change(C, A, [])]), [C]);
  assert.deepEqual(headsOf([genesisObject(A)]), [A], "a genesis nothing rests in yet is the prodrome's one head");
  assert.deepEqual(headsOf([genesisObject(A), change(B, A, []), change(C, A, [B])]), [C]);
});

test("a signature rests on what it signs, a key on nothing, and an epoch on its tips", () => {
  // A key is 64 hex and a signature 128: neither is an edge, and neither is the genesis an object leads with.
  const KEY = name("e");
  const signed: StoredObject = { hash: D, literal: `Signed(object='${C}', key='${KEY}', signature='${KEY}${KEY}')` };
  const added: StoredObject = { hash: D, literal: `KeyAdded(genesis='${A}', actor='ada', key='${KEY}')` };
  const revoked: StoredObject = { hash: D, literal: `KeyRevoked(genesis='${A}', deps=['${B}'], actor='ada', key='${KEY}')` };
  const epoch: StoredObject = { hash: D, literal: `Snapshot(genesis='${A}', tips=['${B}', '${C}'], previous='')` };
  assert.deepEqual(parentsOf(signed), [C]);
  assert.deepEqual(parentsOf(added), []);
  assert.deepEqual(parentsOf(revoked), [B]);
  assert.deepEqual(parentsOf(epoch), [B, C]);
  assert.deepEqual(headsOf([sealed(A, ""), sealed(C, A), signed]), [D], "an act's signature is the head above it");
});

test("a hex-looking string inside the event is not a parent", () => {
  // The cut is at `event=`, so a todo id or a source hash in the record cannot
  // be read as an edge — which is the whole reason the scan is bounded.
  const withHexInside: StoredObject = {
    hash: C,
    literal: `Sealed(prev='${A}', event=Created(todo='alpha', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', text='${B}', note=''))`,
  };
  assert.deepEqual(parentsOf(withHexInside), [A]);
});

test("the heads are the objects nothing rests on", () => {
  assert.deepEqual(headsOf([sealed(A, ""), sealed(B, A)]), [B]);
  // Two branches off one root are two heads, which is exactly what divergence
  // looks like before the box merges.
  assert.deepEqual(headsOf([sealed(A, ""), sealed(B, A), sealed(C, A)]), [B, C]);
  assert.deepEqual(headsOf([]), []);
});

// --- Where an act lands ---------------------------------------------------------

test("appending takes what the core answered, derives the heads, and leaves it unsynced", () => {
  const before = replica({ objects: [sealed(A, ""), sealed(B, A, "beta")], synced: [A, B] });
  // A change on `alpha` rests on alpha's write, not on the replica's head: a second head, and no refusal.
  const after = appended(before, [change(C, A, [A])]);
  assert.deepEqual(after.heads, [B, C]);
  assert.deepEqual(
    unsynced(after).map((object) => object.hash),
    [C],
    "the box has not heard of it yet, which is what the pill counts",
  );
  // Idempotent by hash, like everything else here, and a twin's empty answer changes nothing.
  assert.equal(appended(after, [change(C, A, [A])]), after);
  assert.equal(appended(after, []), after);
});

// --- What to push ---------------------------------------------------------------

test("what to push is everything the box is not known to hold, parents first", () => {
  const held = replica({
    objects: [sealed(A, ""), sealed(B, A), sealed(C, B)],
    synced: [A],
  });
  assert.deepEqual(
    toPush(held, false).map((object) => object.hash),
    [B, C],
    "in the order they are held, so the post is a closure the box can place",
  );
});

test("a replica whose objects the box all holds pushes nothing", () => {
  const held = replica({ objects: [sealed(A, "")], synced: [A] });
  assert.deepEqual(toPush(held, false), []);
});

// --- What to pull ---------------------------------------------------------------

test("a pull asks from the frontier the box's last answer named, never from the heads it holds", () => {
  const held = replica({
    objects: [sealed(A, ""), sealed(B, A), sealed(C, A), sealed(D, B)],
    synced: [A, B, C],
  });
  assert.equal(sinceOf(held), "", "no answer named one yet: from nothing");
  const named = pulled(held, { objects: [], heads: [B, C], untrusted: [], root: null, geneses: [], epoch: 1, since: "Known", frontier: { digest: "e".repeat(64), count: 3 } }, 1);
  assert.equal(sinceOf(named), "e".repeat(64));
  assert.equal(sinceOf(NOTHING), "");
});

test("pulling takes what is new, keeps what is held, and adopts the box's heads", () => {
  const before = replica({ objects: [sealed(A, "")], synced: [A], boxHeads: [A] });
  const after = pulled(before, { objects: [sealed(A, ""), sealed(B, A)], heads: [B], untrusted: ["triage"], root: ROOT, geneses: [], epoch: 1, ...KNOWN }, 1_700_000_000_000);
  assert.deepEqual(
    after.objects.map((object) => object.hash),
    [A, B],
    "an object already held is the same object, not a second copy",
  );
  assert.deepEqual(after.heads, [B]);
  assert.deepEqual(boxHeadsOf(after), [B]);
  assert.deepEqual(untrustedOf(after), ["triage"]);
  assert.equal(rootOf(after), ROOT, "the root the box reads under travels with its policy");
  assert.deepEqual(unsynced(after), [], "everything the box sent is something the box holds");
});

// --- A store the box migrated away from ----------------------------------------------

const genesisObject = (hash: string): StoredObject => ({ hash, literal: `Genesis(schema='inbox', at=datetime(2026, 10, 1, 0, 0, 0))` });

test("a pull drops a prodrome the box no longer holds, and all that rests on it, and keeps the box's own", () => {
  const OLD = name("1");
  const NEW = name("2");
  const before = replica({
    objects: [genesisObject(OLD), change(A, OLD, [OLD]), change(B, OLD, [A]), sealed(C, "")],
    synced: [OLD, A, B, C],
    refused: [{ hash: B, reason: "stale", kind: "final" }],
  });
  const after = pulled(before, { objects: [genesisObject(NEW), change(D, NEW, [NEW])], heads: [D], untrusted: [], root: null, geneses: [NEW, C], epoch: 1, ...KNOWN }, 1_700_000_000_000);
  assert.deepEqual(
    after.objects.map((object) => object.hash),
    [C, NEW, D],
    "the old store's genesis and its changes go; a legacy object and the box's prodrome stay",
  );
  assert.deepEqual(after.heads, [C, D]);
  assert.ok(!after.synced.includes(A) && !after.synced.includes(OLD), "nothing gone is still claimed as the box's");
  assert.deepEqual(after.refused, [], "a refusal of a gone object goes with it");
});

test("a pull that names no geneses says the box holds no prodrome: every prodrome held is forgotten, a legacy object stays", () => {
  const OLD = name("1");
  const before = replica({ objects: [sealed(C, ""), genesisObject(OLD), change(A, OLD, [])], synced: [C, OLD, A] });
  const after = pulled(before, { objects: [], heads: [C], untrusted: [], root: null, geneses: [], epoch: 2, ...KNOWN }, 1_700_000_000_000);
  assert.deepEqual(after.objects.map((object) => object.hash), [C]);
  assert.deepEqual(after.gone, [OLD]);
});

// --- What a receipt teaches ------------------------------------------------------

const receipt = (over: Partial<ReceiptPayload>): ReceiptPayload => ({
  ...KNOWN,
  heads: [],
  adopted: [],
  already: [],
  refused: [],
  ...over,
});

test("adopted and already are the same fact: the box holds it now", () => {
  const before = replica({ objects: [sealed(A, ""), sealed(B, A)] });
  const after = settled(before, receipt({ heads: [B], adopted: [B], already: [A] }), before.generation);
  assert.deepEqual(unsynced(after), []);
  assert.equal(after.box.type, "Unheard", "where the box stands is the pull's to say, from objects this replica holds");
});

test("a refusal is kept with the box's own sentence, and holds back what rests on it", () => {
  const before = replica({
    objects: [sealed(A, ""), sealed(B, A), sealed(C, B)],
    synced: [A],
  });
  const after = settled(
    before,
    receipt({ heads: [A], refused: [{ hash: B, reason: "its event is triage's and you are posting as ada", kind: "final" }] }),
    before.generation,
  );
  const { held, reasons } = stuckBehind(after);
  assert.equal(reasons.length, 1);
  assert.equal(reasons[0]?.reason, "its event is triage's and you are posting as ada");
  assert.equal(held, 2, "the refused object and the one sealed on top of it");
  assert.deepEqual(toPush(after, true), [], "nothing is retried: the box answered, and the answer was no");
});

test("a write refused for its key waits, and goes again once this device is trusted", () => {
  const before = replica({ objects: [sealed(A, ""), sealed(B, A), sealed(C, B)], synced: [A] });
  const after = settled(
    before,
    receipt({ heads: [A], refused: [{ hash: B, reason: "its signatures do not prove it: unsigned", kind: "trust" }] }),
    before.generation,
  );
  assert.deepEqual(stuckBehind(after), { held: 0, reasons: [] }, "nothing is stuck: it waits");
  assert.equal(awaitingTrust(after), 2, "the refused write and the one resting on it");
  assert.deepEqual(toPush(after, false), [], "not posted again while the key is untrusted");
  assert.deepEqual(
    toPush(after, true).map((object) => object.hash),
    [B, C],
    "posted again, parents first, once it is trusted",
  );
});

test("an own write no signature covers is unsigned; a signature or the box holding it covers it", () => {
  const signed: StoredObject = { hash: name("5"), literal: `Signed(object='${C}', key='${"1".repeat(64)}', signature='${"2".repeat(128)}')` };
  const held = replica({ objects: [sealed(A, ""), sealed(B, A), sealed(C, B), signed], synced: [A] });
  assert.deepEqual(
    unsignedWrites(held).map((object) => object.hash),
    [B],
    "the box's own object is not ours to sign, and the signed one is signed",
  );
});

test("a refusal about bytes that were not an object is not kept against anything", () => {
  // There is no name to keep it under and no object of ours it is about, so it
  // is said once and forgotten.
  const after = settled(NOTHING, receipt({ refused: [{ hash: "", reason: "not one canonical object: …", kind: "final" }] }), 0);
  assert.deepEqual(after.refused, []);
});

test("a re-post refused differently replaces the older reason", () => {
  const before = replica({ refused: [{ hash: B, reason: "old", kind: "final" }] });
  const after = settled(before, receipt({ refused: [{ hash: B, reason: "new", kind: "final" }] }), before.generation);
  assert.deepEqual(after.refused, [{ hash: B, reason: "new", kind: "final" }]);
});

// --- What the cards mark ----------------------------------------------------------

test("the todos waiting are the ones an unsynced object names, each once", () => {
  const held = replica({
    objects: [sealed(A, "", "alpha"), sealed(B, A, "alpha"), sealed(C, B, "beta")],
    synced: [A],
  });
  assert.deepEqual([...unsentTodos(held)].sort(), [["alpha", "Sending"], ["beta", "Sending"]]);
});

test("a refused act, and every act resting on it, says refused; one refused for its key waits for trust", () => {
  const objects = [sealed(A, "", "alpha"), sealed(B, A, "alpha"), sealed(C, B, "beta")];
  const refused = replica({ objects, synced: [], refused: [{ hash: A, reason: "no", kind: "final" }] });
  assert.deepEqual([...unsentTodos(refused)].sort(), [["alpha", "Refused"], ["beta", "Refused"]]);
  const waiting = replica({ objects, synced: [A], refused: [{ hash: C, reason: "not trusted yet", kind: "trust" }] });
  assert.deepEqual([...unsentTodos(waiting)].sort(), [["alpha", "Sending"], ["beta", "AwaitingTrust"]]);
});

test("a merge names no todo, so it marks no card", () => {
  // A merge is structure: it carries no event, so there is nothing for it to
  // be waiting ABOUT even while it is waiting.
  const held = replica({ objects: [woven(C, [A, B])], synced: [] });
  assert.deepEqual([...unsentTodos(held)], []);
  assert.equal(unsynced(held).length, 1, "it is still an object the box has not taken");
});

// --- What a kept row reads as ------------------------------------------------------

test("a row in a form an earlier build kept does not read: the reader holds no shim", () => {
  const words = { objects: [sealed(A, "")], heads: [A], boxHeads: [A], synced: [A], refused: [], untrusted: ["triage"], root: null, fetchedAt: 5, gone: [], generation: 0 };
  const kinds = { ...replica({ objects: [sealed(A, ""), sealed(B, A)], synced: [A] }).held, refused: [{ hash: B, reason: "its event is triage's" }] };
  assert.equal(parseReplica(words).type, "Error", "one kept before the box's words were a set");
  assert.equal(parseReplica(kinds).type, "Error", "one kept before refusals had a kind");
});

test("a row as kept now reads back as the replica it was", () => {
  const row = pulled(replica({ objects: [sealed(A, ""), sealed(B, A)], synced: [A] }), { objects: [], heads: [A], untrusted: ["triage"], root: ROOT, geneses: [], epoch: 1, ...KNOWN }, 9);
  const back = lo.withDefault(parseReplica(JSON.parse(JSON.stringify(row.held))), null);
  assert.deepEqual(back, row);
});

test("the box's heads, epoch, policy and root are one word, the last pull's, and none before the box has spoken", () => {
  const first = pulled(NOTHING, { objects: [sealed(A, "")], heads: [A], untrusted: [], root: null, geneses: [], epoch: 1, ...KNOWN }, 1);
  const second = pulled(first, { objects: [sealed(B, A)], heads: [B], untrusted: ["triage"], root: ROOT, geneses: [], epoch: 2, ...KNOWN }, 2);
  assert.deepEqual(second.box, { type: "Heard", word: { epoch: 2, heads: [B], untrusted: ["triage"], root: ROOT, fetchedAt: 2, geneses: [] } });
  assert.deepEqual(NOTHING.box, { type: "Unheard" });
  assert.equal(boxHeadsOf(NOTHING), null, "never heard is not heard to hold nothing");
});

test("an answer of an earlier epoch than a word held is the box going back, and is said", () => {
  const held = pulled(NOTHING, { objects: [sealed(A, "")], heads: [A], untrusted: [], root: null, geneses: [], epoch: 5, ...KNOWN }, 1);
  const answer = (epoch: number) => ({ objects: [], heads: [A], untrusted: [], root: null, geneses: [], epoch, ...KNOWN });
  assert.equal(wentBack(held, answer(4)), true, "a lost state directory");
  assert.equal(wentBack(held, answer(5)), false);
  assert.equal(wentBack(held, answer(6)), false, "a later epoch is the box moving on");
  assert.deepEqual(boxHeadsOf(pulled(held, answer(4), 2)), [A], "and the later word is still the one read");
});
