// A document in the back/forward cache with its streams open holds the sockets the next one needs, so they close when
// it is hidden and open once again when it comes back.

export namespace bfcache {
  export type Window = {
    addEventListener(type: "pagehide", l: () => void): void
    addEventListener(type: "pageshow", l: (e: { readonly persisted: boolean }) => void): void
  }
  export type Kept = { readonly close: () => void; readonly open: () => void }

  // A show that is no return from the cache is a first load, whose opening is the caller's; a show twice opens once.
  export const across = (w: Window, kept: Kept): void => {
    let closed = false
    w.addEventListener("pagehide", () => void (closed || ((closed = true), kept.close())))
    w.addEventListener("pageshow", e => void (e.persisted && closed && ((closed = false), kept.open())))
  }
}
export default bfcache
