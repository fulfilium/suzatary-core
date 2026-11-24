/**
 * `serial`'s law, the one an act needs from a sync: a call is answered by a run
 * that STARTED after the call, so whatever was sealed before calling is in it; a
 * run already in flight read what to push before the act existed.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import serial from "../src/algebra/serial";
import { rng } from "./gen";

/** An effect the test settles by hand; each run reads how many acts were sealed when it started. */
function world() {
  let sealed = 0;
  const inFlight: { readonly read: number; readonly settle: (ok: boolean) => void }[] = [];
  let most = 0;
  const run = (): Promise<number> =>
    new Promise<number>((resolve, reject) => {
      const read = sealed;
      inFlight.push({ read, settle: (ok) => (ok ? resolve(read) : reject(new Error(String(read)))) });
      most = Math.max(most, inFlight.length);
    });
  return {
    run,
    seal: (): number => (sealed += 1),
    settle: (ok: boolean): void => inFlight.shift()?.settle(ok),
    inFlight: (): number => inFlight.length,
    most: (): number => most,
  };
}

/** Every reaction the combinator chains is a few links deep, so sixteen microtask turns let all of them run. */
const settled = async (): Promise<void> => {
  for (let turn = 0; turn < 16; turn += 1) await Promise.resolve();
};

test("a sync asked while one is in flight resolves only after an exchange that carries the act", async () => {
  const w = world();
  const sync = serial.of(w.run);
  void sync();
  await settled();
  const act = w.seal();
  let answered: number | null = null;
  void sync().then((read) => (answered = read));
  w.settle(true);
  await settled();
  assert.equal(answered, null, "the exchange in flight read nothing of the act, so it does not answer the act's sync");
  assert.equal(w.inFlight(), 1, "the queued exchange started once the first settled");
  w.settle(true);
  await settled();
  assert.equal(answered, act, "the exchange that answered it read the act");
});

test("every call is answered by a run that started after it, one run at a time, under any interleaving", async () => {
  for (let seed = 1; seed <= 200; seed += 1) {
    const r = rng(seed * 2654435761);
    const w = world();
    const s = serial.of(w.run);
    const answers: [number, number][] = [];
    let calls = 0;
    for (let step = 0; step < 40; step += 1) {
      if (w.inFlight() > 0 && r.bool(0.4)) {
        w.settle(r.bool(0.8));
      } else {
        const at = w.seal();
        calls += 1;
        s().then(
          (read) => answers.push([at, read]),
          (error: unknown) => answers.push([at, Number((error as Error).message)]),
        );
      }
      await settled();
    }
    while (w.inFlight() > 0) {
      w.settle(true);
      await settled();
    }
    assert.equal(answers.length, calls, `seed ${seed}: every call is answered`);
    for (const [at, read] of answers) assert.ok(read >= at, `seed ${seed}: the act sealed at ${at} was answered by a run that read ${read}`);
    assert.ok(w.most() <= 1, `seed ${seed}: ${w.most()} runs at once`);
  }
});

test("a failed run still lets the queued one start, and a call after both starts afresh", async () => {
  const w = world();
  const s = serial.of(w.run);
  const first = s().catch(() => "failed");
  w.seal();
  const second = s();
  w.settle(false);
  assert.equal(await first, "failed");
  await settled();
  assert.equal(w.inFlight(), 1);
  w.settle(true);
  assert.equal(await second, 1);
  void s();
  assert.equal(w.inFlight(), 1, "idle again, so a call starts a run at once");
});
