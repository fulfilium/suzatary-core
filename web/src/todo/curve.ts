// A fulfillment over a window: the core's samples, and the one line that draws them, coloured by its own value.

import type { Knot } from "../prodrome/core"
import { el, svg } from "../view/dom"
import fulfillment from "./fulfillment"
import time from "../app/time"

export namespace curve {
  const HOUR = 3600
  const DAY = 24 * HOUR

  // Seconds, with now inside [from, to].
  export type Window = { readonly from: number; readonly now: number; readonly to: number }

  export const thirtyDays = (now: number): Window => ({ from: now - 15 * DAY, now, to: now + 15 * DAY })

  export type Sample = { readonly t: number; readonly v: number }
  // Ascending in t.
  export type Samples = readonly [Sample, ...Sample[]]

  // What the core said over a window: the samples before now, and those from now on, the first being now's value.
  export type Drawn = { readonly window: Window; readonly past: readonly Sample[]; readonly ahead: Samples }

  export type Span = { type: "Unsampled" } | { type: "Sampled"; drawn: Drawn }

  export const unsampled: Span = { type: "Unsampled" }
  export const sampled = (drawn: Drawn): Span => ({ type: "Sampled", drawn })
  export const fold = <R>(a: Span, k: { Unsampled: () => R; Sampled: (d: Drawn) => R }): R =>
    a.type === "Sampled" ? k.Sampled(a.drawn) : k.Unsampled()
  // Split at now: with nothing from now on, there is nothing to draw.
  export const of = (w: Window, xs: readonly Sample[]): Span => {
    const [x, ...rest] = xs.filter(p => p.t >= w.now)
    return x === undefined ? unsampled : sampled({ window: w, past: xs.filter(p => p.t < w.now), ahead: [x, ...rest] })
  }
  // A knot that reads ∅ has no number, and a line is never drawn across one: what is drawn is the numbered run through
  // now, so a ∅ before it (a todo not yet priced) starts the line after it, and one after it ends the line there.
  export const ofKnots = (w: Window, ks: readonly Knot[]): Span =>
    of(
      w,
      ks.reduce<{ run: Sample[]; ended: boolean }>(
        ({ run, ended }, [at, v]) => {
          const t = time.parse(at)
          return ended ? { run, ended } : v !== null ? { run: [...run, { t, v }], ended } : t < w.now ? { run: [], ended } : { run, ended: true }
        },
        { run: [], ended: false },
      ).run,
    )

  const meet = (a: Sample, b: Sample, x: number): Sample => ({ t: a.t + ((x - a.v) / (b.v - a.v)) * (b.t - a.t), v: x })

  // When the line first goes below x: now if it already is, never if it stays at or above.
  export const below = (s: Samples, x: number): number | null => {
    const k = s.findIndex(p => p.v < x)
    const [a, b] = [s[k - 1], s[k]]
    return b === undefined ? null : a === undefined ? b.t : meet(a, b, x).t
  }

  const above = (s: Samples, x: number): number | null => {
    const k = s.findIndex(p => p.v >= x)
    const [a, b] = [s[k - 1], s[k]]
    return b === undefined ? null : a === undefined ? b.t : meet(a, b, x).t
  }

  const plural = (n: number, unit: string): string => `${n} ${unit}${n === 1 ? "" : "s"}`
  // Hours up to three days, days past them.
  const span = (dt: number): string => (dt <= 3 * DAY ? plural(Math.round(dt / HOUR), "hour") : plural(Math.round(dt / DAY), "day"))
  const within = (dt: number): string => (dt < HOUR / 2 ? "within the hour" : `in about ${span(dt)}`)

  const stateName = (s: fulfillment.State): string => fulfillment.fold(s, { Problem: "problem", Watch: "watch", Fine: "fine" })

  // "Fulfillment 62% now, watch; problem in about 30 hours": the value, its state now, and the next
  // threshold the value would cross for the worse (or, from Problem, the next it would cross for the better).
  export const label = (d: Drawn): string => {
    const s = d.ahead
    const [now] = s
    const rest = `for the next ${span(d.window.to - d.window.now)}`
    const said = `Fulfillment ${Math.round(now.v * 100)}% now, ${stateName(fulfillment.stateOf(now.v))}`
    const edge = now.v >= fulfillment.FINE ? fulfillment.FINE : now.v >= fulfillment.WATCH ? fulfillment.WATCH : null
    if (edge === null) {
      const up = above(s, fulfillment.WATCH)
      return up === null ? `${said} ${rest}` : `${said}; watch ${within(up - now.t)}`
    }
    const down = below(s, edge)
    return down === null ? `${said} ${rest}` : `${said}; ${edge === fulfillment.FINE ? "watch" : "problem"} ${within(down - now.t)}`
  }

  // The drawing's own units: the frame is W wide and H tall, and stretches to its box.
  export const W = 1000
  export const H = 100

  export const x = (win: Window, w: number) => (t: number): number => ((t - win.from) / (win.to - win.from)) * w
  // A value to a height, 1 at the top.
  export const y = (v: number): number => (1 - v) * H

  // The knots joined by straight lines: the core's own reading between two.
  export const path = (win: Window, ps: readonly Sample[]): string =>
    ps.map((p, i) => `${i === 0 ? "M" : "L"} ${x(win, W)(p.t).toFixed(2)} ${y(p.v).toFixed(2)}`).join(" ")

  // The past runs up to now's value, so the two halves meet.
  export const halves = (d: Drawn): { readonly past: readonly Sample[]; readonly ahead: Samples } => ({
    past: d.past.length === 0 ? [] : [...d.past, d.ahead[0]],
    ahead: d.ahead,
  })

  // A vertical gradient keyed to the value, so every point of the stroke is its own value's colour.
  export const STOPS = 20
  export const stops = (): readonly number[] => Array.from({ length: STOPS + 1 }, (_, i) => i / STOPS)

  const GRADIENT = "curvecolour"
  const WEEK = 7 * DAY

  const gradient = (): SVGLinearGradientElement =>
    svg(
      "linearGradient",
      { id: GRADIENT, gradientUnits: "userSpaceOnUse", x1: 0, y1: y(0), x2: 0, y2: y(1) },
      stops().map(v => svg("stop", { offset: v, "stop-color": fulfillment.colour(v) })),
    )

  const stroke = (win: Window, ps: readonly Sample[], cls: string): SVGPathElement | null =>
    ps.length < 2 ? null : svg("path", { class: `curveline ${cls}`, d: path(win, ps), stroke: `url(#${GRADIENT})` })

  // The owner's day in the reader's locale, short ("Sep 13"): five of them share a phone's width.
  export const dayOf = (t: number, zone: time.Zone): string =>
    new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", timeZone: "UTC" }).format(new Date(`${time.date(zone, t * 1000)}T12:00:00Z`))

  const pct = (f: number): string => `${(f * 100).toFixed(3)}%`

  const date = (at: number, said: string, cls: string): HTMLElement =>
    el("span", { class: `curvedate ${cls}`, style: `left:${pct(at)}` }, said)

  // Thirty days in one line inside a frame from 0 to 100%, the past faded, a line and a dot at now.
  export const line = (d: Drawn, zone: time.Zone): HTMLElement => {
    const { window: w } = d
    const h = halves(d)
    const now = d.ahead[0]
    const across = (t: number): number => (t - w.from) / (w.to - w.from)
    const plot = el(
      "div",
      { class: "curveplot", role: "img", "aria-label": label(d) },
      el("span", { class: "curvehalf", "aria-hidden": "true" }),
      svg(
        "svg",
        { class: "curvesvg", viewBox: `0 0 ${W} ${H}`, preserveAspectRatio: "none", "aria-hidden": "true" },
        svg("defs", null, gradient()),
        stroke(w, h.past, "past"),
        stroke(w, h.ahead, "ahead"),
      ),
      el("span", { class: "curvenow", style: `left:${pct(across(now.t))}` }),
      el("span", { class: "curvedot", style: `left:${pct(across(now.t))};top:${pct(1 - now.v)};background:${fulfillment.colour(now.v)}` }),
    )
    const axis = el(
      "div",
      { class: "curveaxis", "aria-hidden": "true" },
      [1, 0.5, 0].map(v => el("span", { class: "curvetick", style: `top:${pct(1 - v)}` }, String(v * 100))),
    )
    const dates = el(
      "div",
      { class: "curvedates" },
      date(0, dayOf(w.from, zone), "start"),
      date(across(w.now - WEEK), dayOf(w.now - WEEK, zone), "week"),
      date(across(w.now), "now", "now"),
      date(across(w.now + WEEK), dayOf(w.now + WEEK, zone), "week"),
      date(1, dayOf(w.to, zone), "end"),
    )
    return el("div", { class: "curve" }, axis, el("div", { class: "curvebody" }, plot, dates))
  }
}
export default curve
