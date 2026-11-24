// A stream the box keeps open, the reconnect the browser's own: given up after GIVE_UP failures with no open between
// them, or at once when the browser has closed it, and opened anew when the network may be back.

export namespace stream {
  export type Health = { readonly type: "Open" } | { readonly type: "Trying"; readonly failures: number } | { readonly type: "GivenUp" }
  export type Event = { readonly type: "Opened" } | { readonly type: "Failed"; readonly closed: boolean }

  export const GIVE_UP = 3
  export const START: Health = { type: "Trying", failures: 0 }
  const OPEN: Health = { type: "Open" }
  const GIVEN_UP: Health = { type: "GivenUp" }

  const failures = (h: Health): number => (h.type === "Trying" ? h.failures : 0)

  // Given up is final for a socket: the stream comes back as a new one.
  export const step = (h: Health, e: Event): Health =>
    h.type === "GivenUp" ? h
    : e.type === "Opened" ? (h.type === "Open" ? h : OPEN)
    : e.closed || failures(h) + 1 >= GIVE_UP ? GIVEN_UP
    : { type: "Trying", failures: failures(h) + 1 }

  // The part of `EventSource` read here; `CLOSED` spells `EventSource.CLOSED`, so node never needs the global.
  export type Socket = {
    readonly readyState: number
    addEventListener(type: "open" | "error", l: () => void): void
    addEventListener(type: "message", l: (e: { readonly data: string }) => void): void
    close(): void
  }
  const CLOSED = 2

  export type Ends = { readonly frame: (data: string) => void; readonly opened?: () => void; readonly gaveUp?: () => void }

  // The slow tick is for a tab that stays shown and online while the box restarts; `back` opens nothing unless the
  // stream was given up, so the tick is no poll.
  export type Wake = (back: () => void) => () => void

  export const TICK_MS = 30_000

  export const browser: Wake = back => {
    const shown = (): void => void (document.visibilityState === "visible" && back())
    const tick = setInterval(back, TICK_MS)
    window.addEventListener("online", back)
    document.addEventListener("visibilitychange", shown)
    return () => (clearInterval(tick), window.removeEventListener("online", back), document.removeEventListener("visibilitychange", shown))
  }

  // Every open is told, a reconnect's included; a given-up stream is made anew when `wake` says the network may be back.
  export const open = (make: () => Socket, ends: Ends, wake: Wake = browser): (() => void) => {
    let s = make()
    let h: Health = START
    const listen = (at: Socket): void => {
      at.addEventListener("open", () => ((h = step(h, { type: "Opened" })), ends.opened?.()))
      at.addEventListener("message", e => ends.frame(e.data))
      at.addEventListener("error", () => {
        const was = h
        h = step(h, { type: "Failed", closed: at.readyState === CLOSED })
        if (h !== was && h.type === "GivenUp") (at.close(), ends.gaveUp?.())
      })
    }
    listen(s)
    const unwake = wake(() => {
      if (h.type !== "GivenUp") return
      h = START
      s = make()
      listen(s)
    })
    return () => (unwake(), s.close())
  }

  export const of = (url: string) => (): Socket => new EventSource(url)
}
export default stream
