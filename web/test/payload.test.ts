// The generated decoders over every answer the Rust writes (app/api/tests/web/answers.json, a sample of each type a
// route answers): each decodes to itself, a field dropped anywhere is refused naming it, an extra one is dropped, and a
// field of another type is refused. A map's keys are its data, never fields, so the field laws leave them alone.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import d from "../src/wire/decoder"
import payload from "../src/wire/generated/payload"
import { forAll } from "./gen"

type Path = (string | number)[]
type Sample = { value: unknown; maps: Path[] }

const Samples = d.rec(d.arr(d.obj<Sample>({ value: d.unknown, maps: d.arr(d.arr(d.alt(d.str, d.num))) })))
const read = Samples(JSON.parse(readFileSync("app/api/tests/web/answers.json", "utf8")))
if (read.type !== "Val") throw new Error(`answers.json: ${read.type === "Error" ? read.e : "not read"}`)

const decoders = new Map<string, d.Decoder<unknown>>(Object.entries(payload))
const vectors: [string, Sample, d.Decoder<unknown>][] = Object.entries(read.v).flatMap(([name, samples]) => {
  const decode = decoders.get(name)
  if (decode === undefined) throw new Error(`answers.json holds ${name}, which has no decoder`)
  return samples.map((s, i): [string, Sample, d.Decoder<unknown>] => [`${name} ${i}`, s, decode])
})

type At = { path: Path; value: unknown }

// The fields whose JSON is open, kept whole by the decoder: a term's, and an upload's file as a turn holds it.
const OPEN: ReadonlySet<string | number | undefined> = new Set(["spec", "attachment"])

// Every value and where it is, except inside an open field: walked generically, kept whole.
const everywhere = (u: unknown, path: Path = []): At[] => [
  { path, value: u },
  ...(Array.isArray(u)
    ? u.flatMap((x, i) => everywhere(x, [...path, i]))
    : typeof u === "object" && u !== null && !OPEN.has(path.at(-1))
      ? Object.entries(u).flatMap(([k, x]) => everywhere(x, [...path, k]))
      : []),
]

const isObject = (u: unknown): u is Record<string, unknown> => typeof u === "object" && u !== null && !Array.isArray(u)
const same = (a: Path, b: Path) => a.length === b.length && a.every((k, i) => k === b[i])
const isMap = (s: Sample, path: Path) => s.maps.some(m => same(m, path))

const at = (u: unknown, [k, ...rest]: Path, f: (u: unknown) => unknown | undefined): unknown =>
  k === undefined
    ? f(u)
    : Array.isArray(u)
      ? u.map((x, i) => (i === k ? at(x, rest, f) : x))
      : isObject(u)
        ? Object.fromEntries(Object.entries(u).flatMap(([j, x]) => (j !== k ? [[j, x]] : rest.length === 0 && f(x) === undefined ? [] : [[j, at(x, rest, f)]])))
        : u

const fields = (s: Sample) =>
  everywhere(s.value).filter(({ path }) => typeof path.at(-1) === "string" && !isMap(s, path.slice(0, -1)))
const objects = (s: Sample) =>
  everywhere(s.value).filter(({ value, path }) => isObject(value) && !OPEN.has(path.at(-1)) && !isMap(s, path))
const leaves = (s: Sample) => fields(s).filter(({ value }) => ["string", "number", "boolean"].includes(typeof value))
const retyped = (u: unknown): unknown => (typeof u === "boolean" ? "yes" : true)

test("every answer the Rust serialises decodes to itself", () => {
  for (const [name, s, decode] of vectors) assert.deepEqual(decode(s.value), { type: "Val", v: s.value }, name)
})

test("the samples hold a map with entries, so the laws are tested against one", () =>
  assert.ok(vectors.some(([, s]) => s.maps.some(m => everywhere(s.value).some(({ path, value }) => same(path, m) && isObject(value) && Object.keys(value).length > 0)))))

test("law: a field dropped anywhere is refused, naming the field", () =>
  forAll(600, r => {
    const [name, s, decode] = r.pick(vectors)
    const all = fields(s)
    if (all.length === 0) return
    const { path } = r.pick(all)
    const got = decode(at(s.value, path, () => undefined))
    assert.ok(got.type === "Error" && got.e.includes(`missing field "${path.at(-1)}"`), `${name} at ${path.join(".")}`)
  }))

test("law: an extra field anywhere is dropped, never a refusal", () =>
  forAll(600, r => {
    const [name, s, decode] = r.pick(vectors)
    const { path } = r.pick(objects(s))
    const extra = at(s.value, path, o => (isObject(o) ? { ...o, [`extra${r.int(1000)}`]: r.pick([1, "x", null, [], {}]) } : o))
    assert.deepEqual(decode(extra), { type: "Val", v: s.value }, `${name} at ${path.join(".")}`)
  }))

test("law: a field of another type is refused", () =>
  forAll(600, r => {
    const [name, s, decode] = r.pick(vectors)
    const all = leaves(s)
    if (all.length === 0) return
    const { path } = r.pick(all)
    assert.equal(decode(at(s.value, path, retyped)).type, "Error", `${name} at ${path.join(".")}`)
  }))
