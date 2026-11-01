/**
 * HOW LONG A PAGE WAITS ON EACH WRITE (docs/LOCALWRITES-PLAN-2026-11-01.md §1): the box's answer to every write route a
 * page waits on today, timed against the lines' server (`bigstore`'s shape). A measurement, not a test: no gate runs it.
 *
 *   scripts/web-e2e.sh writewait
 *
 * It needs no page. It signs each request with `http.requestName` and `signer.sign`, as `web/src/wire/http.ts` does, with
 * a device key it makes here and trusts at the terminal, as `harness.trustThisDevice` does. Each write runs `RUNS` times;
 * it prints the median and the slowest in milliseconds, and writes them as JSON to `$SUZATARY_WRITEWAIT` when that is set.
 */

import { writeFileSync } from "node:fs"

import { atTheTerminal } from "./harness"
import http from "../src/wire/http"
import signer from "../src/prodrome/signer"

const required = (name: string): string => {
  const v = process.env[name]
  if (v === undefined || v === "") throw new Error(`${name} is unset — run scripts/web-e2e.sh writewait`)
  return v
}

const BASE = required("E2E_BASE_LINES")
const INSTANCE = required("E2E_LINES_INSTANCE")
const RUNS = Number(process.env["SUZATARY_WRITEWAIT_RUNS"] ?? "7")
// The actor the lines' server maps its writer login to (`fixture.rs`, `WRITER_ACTOR`).
const ACTOR = "ada"

const hex = (bytes: ArrayBuffer): string => Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("")

const keyed = async (): Promise<signer.Key> => {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"])) as CryptoKeyPair
  return { public: hex(await crypto.subtle.exportKey("raw", pair.publicKey)), key: pair.privateKey }
}

// A write as the app sends it: stamped now, named by its content, signed by `key`.
const post = async (key: signer.Key, path: string, body: unknown): Promise<{ readonly status: number; readonly json: unknown }> => {
  const init = http.actRequest(body)
  const sent = typeof init.body === "string" ? init.body : ""
  const at = new Date().toISOString()
  const nonce = hex(crypto.getRandomValues(new Uint8Array(16)).buffer)
  const signature = await signer.sign(key, await http.requestName("POST", path, at, nonce, sent, key.public))
  const r = await fetch(BASE + path, {
    ...init,
    headers: { ...init.headers, "X-Suzatary-At": at, "X-Suzatary-Nonce": nonce, "X-Suzatary-Key": key.public, "X-Suzatary-Signature": signature },
  })
  return { status: r.status, json: await r.json().catch(() => null) }
}
const get = async (path: string): Promise<unknown> => (await fetch(BASE + path)).json()

type Timed = { readonly name: string; readonly median: number; readonly slowest: number; readonly statuses: readonly number[] }

// `act` run `RUNS` times in turn, each given its run's number; every answer's status is kept, so a refusal is not timed as a write.
const timed = async (name: string, act: (n: number) => Promise<number>): Promise<Timed> => {
  const ms: number[] = []
  const statuses: number[] = []
  for (let n = 0; n < RUNS; n++) {
    const start = performance.now()
    statuses.push(await act(n))
    ms.push(performance.now() - start)
  }
  const sorted = [...ms].sort((a, b) => a - b)
  return { name, median: Math.round(sorted[Math.floor(sorted.length / 2)] ?? 0), slowest: Math.round(sorted.at(-1) ?? 0), statuses }
}

const frontier = async (path: string): Promise<string> => ((await get(path)) as { frontier: { digest: string } }).frontier.digest

// A topic alias as the core prints one (`secretary::topics`, `Aliased(topic, at, actor, to)`), resting on nothing: each run
// renames a topic no earlier run named, so its register's frontier is empty.
const aliased = (genesis: string, from: string, to: string): string => {
  const d = new Date()
  const at = `datetime(${d.getUTCFullYear()}, ${d.getUTCMonth() + 1}, ${d.getUTCDate()}, ${d.getUTCHours()}, ${d.getUTCMinutes()}, ${d.getUTCSeconds()}, ${d.getUTCMilliseconds() * 1000 + 1})`
  return `Change(genesis='${genesis}', deps=(), event=Aliased(topic='${from}', at=${at}, actor='${ACTOR}', to='${to}'))`
}

const device = await keyed()
atTheTerminal(["device", "trust", device.public], INSTANCE)

const aliases = (await get("/api/stores/topic-aliases/objects")) as { geneses: readonly string[] }
const aliasGenesis = aliases.geneses[0] ?? ""
const opened = (await post(device, "/api/secretary/conversations", { title: "" })).json as { id: string }
const conversation = `/api/secretary/conversations/${encodeURIComponent(opened.id)}`
const linesSince = await frontier("/api/stores/lines/skeleton")

const results: Timed[] = [
  // What every page pull costs with nothing new: the floor under each "then the page pulls" below.
  await timed("pull: lines skeleton, nothing new", async () => (await fetch(`${BASE}/api/stores/lines/skeleton?since=${linesSince}`)).status),
  await timed("pull: inbox objects, whole", async () => (await fetch(`${BASE}/api/stores/inbox/objects`)).status),
  await timed("open a conversation", async () => (await post(device, "/api/secretary/conversations", { title: "" })).status),
  await timed("say (202, before the round runs)", async (n) => (await post(device, `${conversation}/say`, { say: `measured question ${n}`, references: [], attachments: [] })).status),
  await timed("rename a conversation", async (n) => (await post(device, `${conversation}/title`, { title: `Measured ${n}` })).status),
  await timed("rename a topic (topic-aliases objects)", async (n) =>
    (await post(device, "/api/stores/topic-aliases/objects", { objects: [aliased(aliasGenesis, `measured-${Date.now()}-${n}`, `renamed-${n}`)], since: "" })).status),
  // Every tenth bulk proposal stands (`fixture.rs`, `bulk_proposals`): an email card, recorded and not sent by the fixture.
  await timed("tap an email card", async (n) => (await post(device, `/api/secretary/conversations/bulk-proposals/proposals/${n * 10}/tap`, { note: "" })).status),
  await timed("dismiss a card", async (n) => (await post(device, `/api/secretary/conversations/bulk-proposals/proposals/${n * 10 + 100}/dismiss`, { reason: "measured" })).status),
  await timed("author a todo (/api/life/author)", async (n) =>
    (await post(device, "/api/life/author", { add: { body: `Measured todo ${n} ${Date.now()}`, rationale: "", category: "home", priority: { how: "flat", percent: 80 } } })).status),
  await timed("ask to trust a new key (/api/act/key)", async () => {
    const asker = await keyed()
    return (await post(asker, "/api/act/key", { key: asker.public })).status
  }),
  await timed("push token (/api/act/device)", async (n) => (await post(device, "/api/act/device", { platform: "android", token: `measured-${n}` })).status),
]

for (const r of results) console.log(`${r.name.padEnd(44)} median ${String(r.median).padStart(6)} ms  slowest ${String(r.slowest).padStart(6)} ms  ${[...new Set(r.statuses)].join(",")}`)
const out = process.env["SUZATARY_WRITEWAIT"]
if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify(results, null, 2))
