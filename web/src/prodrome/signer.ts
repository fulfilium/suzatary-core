// This device's signing key (SPEC law 42): Ed25519 in WebCrypto, non-extractable, kept in a database of its own so
// resetting the replica never loses it. The seed never leaves the browser; the module only checks and prints.

import idb from "../app/idb"
import lo from "../algebra/lo"
import once from "../algebra/once"

export namespace signer {
  export type Key = { readonly public: string; readonly key: CryptoKey }
  // A key this browser cannot keep, one kept here that does not read (only a new key writes over it), or the key.
  export type Held = { type: "Unsupported"; e: string } | { type: "Unreadable"; e: string } | { type: "Held"; key: Key }

  export const DB = "suzatary-keys"
  const STORE = "keys"
  const DEVICE = "device"
  const ED25519 = { name: "Ed25519" }
  const keys: idb.Db = { name: DB, version: 1, store: STORE, upgrade: db => void db.createObjectStore(STORE) }

  const hex = (bytes: ArrayBuffer): string => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, "0")).join("")

  // What a signature of an object signs, the Prodrome's own prefix before the name.
  export const message = (name: string): Uint8Array<ArrayBuffer> => new TextEncoder().encode(`prodrome object ${name}`)

  export const sign = async (k: Key, name: string): Promise<string> => hex(await crypto.subtle.sign(ED25519, k.key, message(name)))

  const isKey = (u: unknown): u is Key =>
    typeof u === "object" && u !== null && "public" in u && typeof u.public === "string" && "key" in u && u.key instanceof CryptoKey

  const parse = (u: unknown): lo.Settled<Key> => (isKey(u) ? lo.val(u) : lo.error("it is not a key this browser made"))

  // Minted and kept: the private half is non-extractable, so no page, extension or script can read it out.
  const mint = async (): Promise<Key> => {
    const pair = await crypto.subtle.generateKey(ED25519, false, ["sign", "verify"])
    if (!("privateKey" in pair)) throw new Error("Ed25519 answered no key pair")
    const key = { public: hex(await crypto.subtle.exportKey("raw", pair.publicKey)), key: pair.privateKey }
    await idb.within(keys, "readwrite", s => idb.done(s.put(key, DEVICE)))
    return key
  }

  // Only a device that keeps no key mints one: a row that does not read is said, and only `renew` writes over it. A
  // store that does not open is a failure, so it is asked again: a browser resumed from sleep may open it next time.
  export const keyOf = async (row: idb.Row<Key>, minted: () => Promise<Key>): Promise<Held> =>
    row.type === "Held" ? { type: "Held", key: row.v }
    : row.type === "Absent" ? { type: "Held", key: await minted() }
    : row.type === "Unreadable" ? { type: "Unreadable", e: `the key kept here does not read (${row.why})` }
    : Promise.reject(new Error(`this browser does not open its storage (${row.why})`))

  // The key this page signs with, once it has read or minted one, and the key this device keeps now, read afresh and
  // never minted: another tab's wipe and the new device after it put another there, or none.
  export type Here = { type: "None" } | { type: "Held"; key: string }
  export type Kept = { type: "Absent" } | { type: "Kept"; key: string }
  let here: Here = { type: "None" }
  export const signing = (): Here => here

  export const kept = async (): Promise<Kept> => {
    if (!idb.available()) return { type: "Absent" }
    const row = await idb.read(keys, DEVICE, parse)
    return row.type === "Held" ? { type: "Kept", key: row.v.public } : { type: "Absent" }
  }

  // The key this device keeps now, read and never minted: what the core's worker signs with, so it never races the page
  // to mint one.
  export const existing = async (): Promise<Key | null> => {
    if (!idb.available()) return null
    const row = await idb.read(keys, DEVICE, parse)
    return row.type === "Held" ? row.v : null
  }

  const holding = (h: Held): Held => ((here = h.type === "Held" ? { type: "Held", key: h.key.public } : here), h)

  const load = async (): Promise<Held> =>
    !idb.available() || typeof crypto === "undefined" || crypto.subtle === undefined
      ? { type: "Unsupported", e: "this browser keeps no keys" }
      : holding(await keyOf(await idb.read(keys, DEVICE, parse), mint))

  // One key per device, kept once read; a browser without Ed25519, or one whose storage does not open, says so, signs
  // nothing, and is asked again next time.
  const loaded = once.of(load)
  export const held = (): Promise<Held> => loaded.get().catch((e: unknown): Held => ({ type: "Unsupported", e: `no Ed25519 key here: ${lo.reason(e)}` }))

  // A new key in the old one's place, for a device whose key was revoked or does not read: its storage, replica and acts
  // are kept, and the old key is gone from this browser, so nothing it signed can be signed again here.
  const renewed = new Set<() => void>()
  // Wakes when this device's key is renewed; returns its own removal.
  export const watch = (listener: () => void): (() => void) => (renewed.add(listener), () => void renewed.delete(listener))

  export const renew = async (): Promise<Held> => {
    const fresh: Held = holding({ type: "Held", key: await mint() })
    loaded.forget()
    renewed.forEach(l => l())
    return fresh
  }
}
export default signer
