// Usage: what this box has spent of its Claude allowance, in the two windows the limits measure.

import { usage as ask } from "../wire/api"
import data from "../algebra/data"
import type { UsageAllowance, UsagePayload, UsageRefusal, UsageScope, UsageTokens, UsageWindow } from "../wire/generated/wire"
import empty from "../view/empty"
import type { Child } from "../view/dom"
import { el } from "../view/dom"
import { pct, percent, when } from "../view/format"
import pie from "../view/pie"
import frame from "../view/frame"
import asked from "../app/asked"
import time from "../app/time"
import app from "../app/app"

export namespace usage {
  // `left` is the share of the allowance still unspent, a judgement: the less is left, the nearer the scale's bad end.
  export type Described = { readonly text: string; readonly left: number | null }

  // Past the allowance with overage on, tokens are money: said as a quantity, never a price.
  export namespace credits {
    export type Credits =
      | { readonly type: "disabled"; readonly reason: string }
      | { readonly type: "spent"; readonly since: string; readonly tokens: UsageTokens }
      | { readonly type: "unspent" }

    export const disabled = (reason: string): Credits => ({ type: "disabled", reason })
    export const spent = (since: string, tokens: UsageTokens): Credits => ({ type: "spent", since, tokens })
    export const unspent: Credits = { type: "unspent" }

    export const of = (w: UsageWindow, r: UsageRefusal): Credits =>
      !r.overage_enabled ? disabled(r.overage_reason)
      : w.on_credits !== null ? spent(r.at, w.on_credits)
      : unspent

    export const fold = <R>(
      alg: { disabled: (reason: string) => R; spent: (since: string, t: UsageTokens) => R; unspent: () => R },
      c: Credits,
    ): R => (c.type === "disabled" ? alg.disabled(c.reason) : c.type === "spent" ? alg.spent(c.since, c.tokens) : alg.unspent())
  }

  namespace allowance {
    type Reported = Extract<UsageAllowance, { kind: "reported" }>
    type RateLimited = Extract<UsageAllowance, { kind: "rate_limited" }>
    type Refused = Extract<UsageAllowance, { kind: "refused" }>

    export const fold = <R>(
      alg: { reported: (a: Reported) => R; rateLimited: (a: RateLimited) => R; refused: (a: Refused) => R; unknown: () => R },
      a: UsageAllowance,
    ): R =>
      a.kind === "reported" ? alg.reported(a)
      : a.kind === "rate_limited" ? alg.rateLimited(a)
      : a.kind === "refused" ? alg.refused(a)
      : alg.unknown()
  }

  const BIG = 22

  export const compact = (n: number): string =>
    n < 1000 ? String(n)
    : n < 1_000_000 ? `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`
    : n < 1_000_000_000 ? `${(n / 1_000_000).toFixed(1)}M`
    : `${(n / 1_000_000_000).toFixed(2)}B`

  export const total = (t: UsageTokens): number => t.fresh + t.cache_write + t.cache_read + t.output

  const at = (stamp: string): string => {
    const ms = time.ms(stamp)
    return ms === null ? stamp : when(ms, app.now().zone)
  }

  // "" once it has passed.
  export const until = (stamp: string, nowMs: number): string => {
    const ms = (time.ms(stamp) ?? nowMs) - nowMs
    if (ms <= 0) return ""
    const m = Math.round(ms / 60_000)
    const h = Math.floor(m / 60)
    return m < 60 ? `in ${m}m` : h < 48 ? `in ${h}h ${m % 60}m` : `in ${Math.floor(h / 24)}d ${h % 24}h`
  }

  const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

  // A share left only under "reported": a guessed ceiling is not an allowance. Past it, nothing is left.
  export const describeAllowance = (w: UsageWindow, nowMs: number): Described =>
    allowance.fold<Described>(
      {
        reported: a => {
          const reset = a.resets_at === null ? "" : until(a.resets_at, nowMs)
          return { left: Math.max(0, 1 - a.utilization), text: `as of ${at(a.as_of)}${reset === "" ? "" : `, resets ${reset}`}.` }
        },
        rateLimited: a => {
          const minutes = Math.max(1, Math.round(a.retry_after_seconds / 60))
          const last =
            a.last_reported === null
              ? "it has never answered"
              : `last answer ${at(a.last_reported.as_of)}: ${pct(a.last_reported.utilization)} used`
          return {
            left: null,
            text: `the allowance endpoint is rate-limiting this account (retry in ${minutes} min); ${last}.`,
          }
        },
        refused: a => ({
          left: null,
          text:
            a.status === 403 && a.words.includes("scope")
              ? `the allowance is unread: the usage endpoint refuses this box's token (403 ${a.words}). The box signs in with a setup-token, which cannot carry the user:profile scope the endpoint asks for, and it is not handed the claude.ai login's token, which would give the server far more than a number to show.`
              : `the allowance is unread: the usage endpoint refused this box's token (${a.status} ${a.words}) ${at(a.at)}.`,
        }),
        unknown: () => ({ left: null, text: "the allowance has not been read yet." }),
      },
      w.allowance,
    )

  // "" when the box has no opinion, and the page then draws nothing.
  export const describeScope = (s: UsageScope): string => {
    const known = s.counted.length + s.not_counted.length
    if (known === 0) return ""
    const base = `This page counts ${s.counted.length} of ${known} project ${known === 1 ? "directory" : "directories"}: ${s.counted.join(", ")}`
    return s.not_counted.length === 0 ? `${base}.` : `${base} · not counted: ${s.not_counted.join(", ")}.`
  }

  const tokens = (t: UsageTokens): HTMLElement => {
    const rows: [string, number][] = [
      ["fresh", t.fresh],
      ["cache write", t.cache_write],
      ["cache read", t.cache_read],
      ["output", t.output],
    ]
    return el("dl", { class: "usagetokens" }, rows.map(([k, v]) => [el("dt", null, k), el("dd", null, compact(v))]))
  }

  const headline = (w: UsageWindow, nowMs: number): HTMLElement => {
    const d = describeAllowance(w, nowMs)
    return el(
      "div",
      { class: "usagehead" },
      d.left === null
        ? el("div", { class: "usagebig" }, `${compact(total(w.tokens))} tokens`)
        : el("div", { class: "usagebig" }, pie.view(d.left, pie.judgement, `${percent(d.left)} of the allowance left`, BIG), `${pct(d.left)} of the allowance left`),
      el("p", { class: "usagenote" }, capital(d.text)),
    )
  }

  // A floor of the real limit: a plain count beside its date, never divided into anything.
  const mostSeen = (w: UsageWindow): Child =>
    w.most_seen_before_refusal === null || w.last_refusal === null
      ? null
      : el(
          "p",
          { class: "usagenote usagemostseen" },
          `Most seen before a refusal on ${at(w.last_refusal.at)}: ${compact(total(w.most_seen_before_refusal))} tokens.`,
        )

  const elsewhere = (w: UsageWindow): Child =>
    total(w.elsewhere) === 0
      ? null
      : el(
          "p",
          { class: "usagenote usageelsewhere" },
          `Elsewhere on this box, this window: ${compact(total(w.elsewhere))} tokens, in project directories that are neither this session's nor this repository's automated one.`,
        )

  const creditsView = (c: credits.Credits): HTMLElement =>
    credits.fold<HTMLElement>(
      {
        disabled: reason =>
          el(
            "div",
            { class: "usagecredits" },
            el("div", { class: "usagecreditshead" }, "Not on credits"),
            el(
              "p",
              null,
              reason === ""
                ? "Overage is disabled at the organisation — nothing runs on credits."
                : `Overage is disabled at the organisation (${reason}) — nothing runs on credits.`,
            ),
          ),
        spent: (since, t) =>
          el(
            "div",
            { class: "usagecredits usagecredits-on" },
            el("div", { class: "usagecreditshead" }, "On credits"),
            el(
              "p",
              null,
              `Extra usage is on. Since the allowance refused a request ${at(since)}, this box has spent ${compact(total(t))} tokens past it — that is real money. The rate is not on this box, so no price is shown.`,
            ),
            tokens(t),
          ),
        unspent: () =>
          el(
            "div",
            { class: "usagecredits" },
            el("div", { class: "usagecreditshead" }, "Not on credits"),
            el("p", null, "Overage is enabled, but nothing has been spent past the allowance yet."),
          ),
      },
      c,
    )

  const refusal = (r: UsageRefusal, nowMs: number): HTMLElement => {
    const left = r.resets_at === null ? "" : until(r.resets_at, nowMs)
    const reset = r.resets_at === null ? "" : left === "" ? ` It reset ${at(r.resets_at)}.` : ` It resets ${left}.`
    return el("p", { class: "usagerefusal" }, `Last refused ${at(r.at)}.${reset}`)
  }

  const share = (f: number): Child => [pie.view(f, pie.neutral, `${percent(f)} of the tokens`), " ", pct(f)]

  const shares = (w: UsageWindow): HTMLElement => {
    const whole = total(w.tokens)
    const rows = [
      ...w.by_who.map(r => ({ label: r.who, tokens: r.tokens })),
      ...w.by_model.map(r => ({ label: r.model, tokens: r.tokens })),
    ]
    return el(
      "table",
      { class: "usageshares" },
      el(
        "tbody",
        null,
        rows.map(r =>
          el(
            "tr",
            null,
            el("td", { class: "usagelabel" }, r.label),
            el("td", { class: "usagenum" }, compact(total(r.tokens))),
            el("td", { class: "usagenum" }, whole === 0 ? "" : share(total(r.tokens) / whole)),
          ),
        ),
      ),
    )
  }

  export const windowCard = (w: UsageWindow, title: string, nowMs: number): HTMLElement =>
    el(
      "section",
      { class: "usagewindow" },
      el("h2", { class: "usagetitle" }, title),
      headline(w, nowMs),
      el(
        "p",
        { class: "usagenote" },
        `${w.requests} request${w.requests === 1 ? "" : "s"} since ${at(w.since)}` +
          (w.cache_hit === null ? "" : `, ${pct(w.cache_hit)} of input served from cache`) +
          ".",
      ),
      tokens(w.tokens),
      w.by_who.length === 0 && w.by_model.length === 0 ? null : shares(w),
      elsewhere(w),
      mostSeen(w),
      w.last_refusal === null ? null : [refusal(w.last_refusal, nowMs), creditsView(credits.of(w, w.last_refusal))],
    )

  export const view = (p: UsagePayload): Child => {
    const nowMs = time.ms(p.at) ?? NaN
    const scope = describeScope(p.scope)
    const seen = `${compact(total(p.seen))} tokens over ${p.seen_requests} requests. Folded ${at(p.at)}.`
    return el(
      "div",
      { class: "usagebody" },
      p.seen_requests === 0
        ? empty.note(
            p.seen_since === null
              ? "No transcripts on this box yet, so nothing to count."
              : `No requests since ${at(p.seen_since)}, which is as far back as the reader looks.`,
          )
        : [
            windowCard(p.five_hour, "Last 5 hours", nowMs),
            windowCard(p.seven_day, "Last 7 days", nowMs),
            el(
              "p",
              { class: "usagenote usagefoot" },
              p.seen_since === null
                ? `All time on this box: ${seen}`
                : `Since ${at(p.seen_since)}, as far back as the reader looks: ${seen}`,
            ),
            scope === "" ? null : el("p", { class: "usagenote usagescope" }, scope),
          ],
    )
  }

  // The page is the same while what it prints is: its instants are drawn to the minute, and the box stamps them to the
  // second it was asked.
  const minute = (stamp: string): string | number => {
    const ms = time.ms(stamp)
    return ms === null ? stamp : Math.floor(ms / 60_000)
  }
  const drawn = (p: UsagePayload): unknown => ({
    ...p,
    at: minute(p.at),
    seen_since: p.seen_since === null ? null : minute(p.seen_since),
    five_hour: { ...p.five_hour, since: minute(p.five_hour.since) },
    seven_day: { ...p.seven_day, since: minute(p.seven_day.since) },
  })
  const spent = asked.question(ask, (a, b) => data.same(drawn(a), drawn(b)))

  export const page: frame.Page<UsagePayload> = {
    className: "usagepage",
    title: "Usage",
    now: () =>
      el(
        "p",
        { class: "legend" },
        "What this box has spent of its Claude allowance, in the two windows the limits measure. Tokens, never a price.",
      ),
    reads: () => spent,
    view,
  }

  export const mount = (root: HTMLElement) => frame.open(root, page)
}
export default usage
