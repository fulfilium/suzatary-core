// Instants and the owner's days. An instant is a point in time, written naive and UTC by definition, as every `at` on
// the chain is; an explicit `Z` or offset (a mail's date carries one) is honoured. A day is the owner's, read in the
// zone the instance names; this device's zone never enters either.

export namespace time {
  const MINUTE_MS = 60_000
  const HOUR_MS = 60 * MINUTE_MS
  const DAY_MS = 24 * HOUR_MS

  // A named zone, `America/New_York`, as `/api/me` hands it over: the owner's, which moves when he does.
  export type Zone = { readonly name: string }

  export const UTC: Zone = { name: "UTC" }

  /** Epoch milliseconds: the one read of the clock. */
  export const now = (): number => Date.now()

  const SHAPE = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?)?(Z|[+-]\d{2}:?\d{2})?$/

  const offsetMs = (z: string | undefined): number =>
    z === undefined || z === "Z" ? 0 : (z[0] === "-" ? -1 : 1) * (Number(z.slice(1, 3)) * 60 + Number(z.slice(-2))) * MINUTE_MS

  /** The one reading of a written instant: epoch milliseconds, or null for a string that names none. */
  export const ms = (s: string): number | null => {
    const m = SHAPE.exec(s.trim())
    if (m === null) return null
    const n = (i: number): number => Number(m[i] ?? 0)
    const [y, mo, d, h, mi, sec] = [n(1), n(2), n(3), n(4), n(5), n(6)]
    const t = Date.UTC(y, mo - 1, d, h, mi, sec, Number(`${m[7] ?? ""}000`.slice(0, 3)))
    const back = new Date(t)
    // `Date.UTC` carries an overflow over (the 31st of April is the 1st of May); a string that needs it names nothing.
    const real = back.getUTCFullYear() === y && back.getUTCMonth() === mo - 1 && back.getUTCDate() === d && h < 24 && mi < 60 && sec < 60
    return real ? t - offsetMs(m[8]) : null
  }

  /** Epoch seconds of an instant; a string that is not one is an error. */
  export const parse = (s: string): number => {
    const t = ms(s)
    if (t === null) throw new Error(`not an ISO instant: ${s}`)
    return t / 1000
  }

  const naive = (t: number): string => new Date(t).toISOString().slice(0, 23)

  /** Naive and UTC, to the second: the shape `fpl::parse_iso` reads. */
  export const instant = (seconds: number): string => naive(seconds * 1000).slice(0, 19)

  // To the millisecond, what a replica folds and seals at: truncated to the second, "now" would be up to a second
  // in the past, and an event the box authored a moment ago (it carries microseconds) would not yet be believed.
  export const moment = (t: number = now()): string => naive(Math.floor(t))

  const faces = new Map<string, Intl.DateTimeFormat>()
  const face = (z: Zone): Intl.DateTimeFormat => {
    const known = faces.get(z.name)
    if (known !== undefined) return known
    const made = new Intl.DateTimeFormat("en-US", {
      timeZone: z.name, hourCycle: "h23", year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
    })
    faces.set(z.name, made)
    return made
  }

  // A name is a zone only where this browser's Intl reads it as one; the face it makes is the one every reading of the
  // zone uses after, so the browser loads the zone's rules once.
  export const zoneOf = (name: string): Zone | null => {
    try {
      face({ name })
      return { name }
    } catch {
      return null
    }
  }

  // The owner's wall clock at `t`, as an instant whose UTC fields read it.
  const wall = (z: Zone, t: number): number => {
    const parts = face(z).formatToParts(t)
    const part = (type: string): number => Number(parts.find(p => p.type === type)?.value ?? NaN)
    return Date.UTC(part("year"), part("month") - 1, part("day"), part("hour"), part("minute"), part("second"), ((t % 1000) + 1000) % 1000)
  }

  /** The owner's day at `t`, `YYYY-MM-DD`: what a date input's `min` and a deadline compare against. */
  export const date = (z: Zone, t: number = now()): string => naive(wall(z, t)).slice(0, 10)

  /** When the owner's `day` begins: the first minute whose wall clock reads it, so a day whose midnight a clock change skips begins when its clock does. */
  export const start = (z: Zone, day: string): number => {
    const midnight = ms(day) ?? NaN
    const guess = midnight - (wall(z, midnight) - midnight)
    let [lo, hi] = [guess - 3 * HOUR_MS, guess + 3 * HOUR_MS]
    while (hi - lo > MINUTE_MS) {
      const mid = lo + Math.floor((hi - lo) / 2 / MINUTE_MS) * MINUTE_MS
      if (date(z, mid) >= day) hi = mid
      else lo = mid
    }
    return hi
  }

  /** When the owner's next day begins after `t`. */
  export const nextDay = (z: Zone, t: number): number => start(z, naive((ms(date(z, t)) ?? NaN) + DAY_MS).slice(0, 10))

  /** `t` on the owner's clock, to the minute: `2026-09-04 11:32`. */
  export const minute = (z: Zone, t: number): string => naive(wall(z, t)).slice(0, 16).replace("T", " ")

  /** Whole days from the owner's day `from` to `to`, both `YYYY-MM-DD`; null where either is no day. */
  export const daysBetween = (from: string, to: string): number | null => {
    const [a, b] = [ms(from), ms(to)]
    return a === null || b === null ? null : Math.round((b - a) / DAY_MS)
  }
}
export default time
