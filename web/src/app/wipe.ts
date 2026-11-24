// A device whose key was revoked deletes what it holds: every database of this origin (the replica, the rendered
// text, the key), every cache of it (each build's, the content cache, the build last heard) and the browser's key-value
// storage, then loads afresh as a new device. A wipe is marked in that storage before it deletes anything. Every other
// tab hears the mark and, by the mark as it reads it then, wipes too or loads afresh, so no tab writes the revoked
// device's acts back; a page that loads with the mark finishes the wipe, so a reload mid-wipe leaves no new key beside
// the old device's storage.

import { DB_NAME } from "../prodrome/objects"
import signer from "../prodrome/signer"
import kv from "./kv"
import rendered from "./rendered"

export namespace wipe {
  // The key-value storage's record that a wipe has begun and not ended.
  export const MARK = "suzatary.wiping"

  export type Mark = { type: "Clear" } | { type: "Begun" }

  export const markOf = (value: string | null): Mark => (value === null ? { type: "Clear" } : { type: "Begun" })

  // Why this page wipes: the box answered a request with the wipe, another tab marked one, or this page loaded with a
  // wipe's mark that a reload or a failed deletion left.
  export type Why = "Gone" | "Told" | "Resumed"

  export type Effects = {
    readonly seal: () => void
    readonly databases: () => Promise<readonly string[]>
    readonly drop: (name: string) => Promise<void>
    readonly caches: () => Promise<readonly string[]>
    readonly uncache: (name: string) => Promise<void>
    readonly forget: () => void
    readonly mark: () => void
    readonly marked: () => Mark
    readonly reload: () => void
    readonly signing: () => signer.Here
    readonly kept: () => Promise<signer.Kept>
    // A finished wipe that failed again: this page stays sealed, and says it could not delete what it holds.
    readonly stuck: () => void
  }

  // The databases this build opens, whatever `indexedDB.databases()` can list: the replica, the rendered text, the key.
  // Read when asked, since the rendered store reaches this module through the wire.
  export const known = (): readonly string[] => [DB_NAME, rendered.DB, signer.DB]

  // Sealed first: a write still in flight would otherwise open a database again after it was deleted. The storage is
  // cleared and marked before the first wait, and cleared again after the last: a page that writes to it while the
  // databases go keeps nothing, and a page that stops before the end leaves the mark for the next load. A database that
  // would not go keeps the mark too, so the next load tries again; a page that was itself finishing a marked wipe then
  // stays sealed rather than reloading into the same failure.
  export const all = async (fx: Effects, why: Why): Promise<void> => {
    fx.seal()
    fx.forget()
    fx.mark()
    const listed = await fx.databases().catch(() => [])
    const dropped = await Promise.all([...new Set([...known(), ...listed])].map(n => fx.drop(n).then(() => true, () => false)))
    await Promise.all((await fx.caches().catch(() => [])).map(n => fx.uncache(n).catch(() => {})))
    fx.forget()
    if (dropped.every(d => d)) return fx.reload()
    fx.mark()
    return why === "Resumed" ? fx.stuck() : fx.reload()
  }

  const settled = (r: IDBOpenDBRequest): Promise<void> =>
    new Promise((ok, no) => {
      // Never done at `blocked`: a connection still open (this page's own, mid-operation, or another tab's, which hears
      // the mark and wipes too) holds it, and a page that reloads then takes the pending deletion with it.
      r.onsuccess = () => ok()
      r.onerror = () => no(new Error(String(r.error)))
    })

  export const browser: Omit<Effects, "stuck"> = {
    seal: () => {
      if (typeof IDBFactory === "undefined") return
      IDBFactory.prototype.open = () => {
        throw new DOMException("this device is deleting what it holds", "InvalidStateError")
      }
    },
    databases: async () =>
      typeof indexedDB === "undefined" || typeof indexedDB.databases !== "function"
        ? []
        : (await indexedDB.databases()).flatMap(d => (d.name === undefined ? [] : [d.name])),
    drop: name => (typeof indexedDB === "undefined" ? Promise.resolve() : settled(indexedDB.deleteDatabase(name))),
    caches: () => (typeof caches === "undefined" ? Promise.resolve([]) : caches.keys()),
    uncache: name => caches.delete(name).then(() => undefined),
    forget: () => {
      for (const s of [() => window.localStorage, () => window.sessionStorage]) {
        try {
          s().clear()
        } catch {
          // A browser that keeps no storage has none to clear.
        }
      }
    },
    mark: () => kv.local.set(MARK, "1"),
    marked: () => markOf(kv.local.get(MARK)),
    reload: () => window.location.reload(),
    signing: signer.signing,
    kept: signer.kept,
  }

  // Whether the key this page signs with is no longer this device's: a wipe ended and a new device began while the
  // page was cached. A page that holds no key yet has signed nothing to compare.
  export const replaced = (here: signer.Here, kept: signer.Kept): boolean =>
    here.type === "Held" && (kept.type === "Absent" || kept.key !== here.key)

  // What a page does about wipes, at most once: the first wipe reloads it or leaves it sealed, and every word after
  // that is moot.
  export type Page = {
    // The box answered a request with the wipe.
    readonly gone: () => void
    // At load, before anything opens a database: a wipe a reload cut short, or one a database would not leave.
    readonly resume: () => void
    // Another tab changed the mark. The mark is read now, not off the event, which a page frozen in the background or
    // kept for back and forward hears late: set, the wipe runs here too; clear, it is over, and this page holds the old
    // device's replica in memory, so it is sealed and loads afresh, deleting nothing of the new device.
    readonly heard: () => void
    // Shown again from the back and forward cache: a wipe still marked runs here. One already over left another key, or
    // none, where this page's was (a browser need not deliver the storage event queued for a cached page), so the page
    // is sealed and loads afresh, deleting nothing; a page shown again with its own key still kept keeps its place.
    readonly shown: () => Promise<void>
  }

  export const page = (fx: Effects): Page => {
    let asked = false
    const once = (act: () => void): void => {
      if (asked) return
      asked = true
      act()
    }
    const wipe = (why: Why): void => once(() => void all(fx, why))
    return {
      gone: () => wipe("Gone"),
      resume: () => (fx.marked().type === "Begun" ? wipe("Resumed") : undefined),
      heard: () => (fx.marked().type === "Begun" ? wipe("Told") : once(() => (fx.seal(), fx.reload()))),
      shown: async () => {
        if (fx.marked().type === "Begun") return wipe("Told")
        if (replaced(fx.signing(), await fx.kept())) once(() => (fx.seal(), fx.reload()))
      },
    }
  }
}
export default wipe
