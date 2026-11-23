// The page holds no replica of a chain: the core's worker owns every one. The law reads esbuild's metafile of the page's
// bundle, built as `scripts/build-web.sh` builds it from `main.ts` with every chunk it splits off, and of the worker's.
// What it proves: no module the page loads is the worker's chains (`chainsync.ts`, which holds the earlier builds' row
// decoder), its engines or a page-side replica, and the worker's bundle holds them; and no module the page loads but
// `core.ts`, which declares them, constructs the core's `Chain` or `Chains` or calls a function that reads one. The page
// does load the core, for the curves it draws, so the second half is what says it builds no chain of its own.

import { test } from "node:test"
import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { mkdtempSync, readFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

const inputsOf = (entry: string, split: boolean): string[] => {
  const out = mkdtempSync(join(tmpdir(), "bundle-"))
  execFileSync("esbuild", [
    entry,
    "--bundle",
    ...(split ? ["--splitting", `--outdir=${out}`] : [`--outfile=${join(out, "out.js")}`]),
    "--format=esm",
    "--target=es2022",
    "--define:__DEV__=false",
    "--define:__CONTENT__={}",
    "--external:./prodrome.js",
    "--external:./typst.js",
    "--external:./views.js",
    `--metafile=${join(out, "meta.json")}`,
    "--log-level=error",
  ], { encoding: "utf8" })
  return Object.keys((JSON.parse(readFileSync(join(out, "meta.json"), "utf8")) as { inputs: Record<string, unknown> }).inputs)
}

const WORKERS = ["web/src/prodrome/chainsync.ts", "web/src/prodrome/replicasync.ts", "web/src/prodrome/questions.ts", "web/src/prodrome/storeset.ts"]

test("law: no module of the page's bundle is a replica of a chain, the worker's chains or their engines", () => {
  const page = inputsOf("web/src/main.ts", true)
  assert.ok(page.includes("web/src/prodrome/sync.ts"), "the page's bundle is read")
  assert.deepEqual(page.filter(f => WORKERS.includes(f) || /\/replica\.ts$/.test(f)), [])
  const worker = inputsOf("web/src/prodrome/worker.ts", false)
  assert.deepEqual(WORKERS.filter(f => !worker.includes(f)), [], "the worker's bundle holds them")
})

// A construction of the core's chain or chains, or a call of a function of `core.ts` that reads one.
const BUILDS_A_CHAIN = /new\s+\w+\.Chains?\s*\(|\.hold_chain\s*\(|\b(holdIn|policyOf|boxHeadsIn|unsentIn|orderIn)\s*\(/

test("law: no module of the page's bundle but the core's declarations builds a chain or reads one through the core", () => {
  const page = inputsOf("web/src/main.ts", true).filter(f => f.startsWith("web/src/") && f !== "web/src/prodrome/core.ts")
  assert.deepEqual(page.filter(f => BUILDS_A_CHAIN.test(readFileSync(f, "utf8"))), [])
  const worker = inputsOf("web/src/prodrome/worker.ts", false).filter(f => f.startsWith("web/src/") && f !== "web/src/prodrome/core.ts")
  assert.ok(worker.some(f => BUILDS_A_CHAIN.test(readFileSync(f, "utf8"))), "the pattern finds the worker's")
})
