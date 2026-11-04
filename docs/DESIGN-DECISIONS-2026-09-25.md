# Suzatary × bmabsout: final design decisions

Agreed between the Suzatary session and the bmabsout design-system session, 2026-09-25. This file replaces the open points in `design-review.md`. Values are for light / dark unless marked as the same in both.

## A. State colours: changed (all three, colour-only marks, 2026-09-26)

**Superseded by §F (2026-09-28):** a mark's colour is its value's, on one
continuous gradient; Problem, Watch and Fine are words, sorting and
thresholds, never colours, and the three tokens below are gone from
`design/tokens.json`. Kept as the record of what was decided.

| State | Range | Final colour |
|---|---|---|
| Problem | fulfillment < 0.5 | `#e55900` (both themes) |
| Watch | 0.5 – 0.7 | `#a44554` (both themes) |
| Fine | ≥ 0.7 | `#379775` (both themes) |
| Pie remainder | the unfilled part | `#e7e3e3` / `#312d2c` (`grid`) |

Superseded by the colour-only redesign (§D below, 2026-09-26): the line/area
fuse this section originally validated is gone, so the checks against a
card-surface backdrop no longer apply as written. The three values are the
design system's own board — `design/tokens.json`'s `problem`/`watch`/`fine`,
the one source `scripts/gen-tokens.mjs` regenerates `web/static/tokens.css`
and `tokens.typ` from.

**Rule:** state colours mark state only. Never use them for text, and never put them beside the design system's `chart-*` series colours.

## B. Surfaces: agreed (one sheet per group), with two additions

**The group sheet:**
- Background `#ffffffbf` / `#1b1718bf`, `backdrop-filter: blur(0.6rem)`, `border-radius: 8px`.
- No border, no shadow.
- Padding: `0 1rem 0.75rem`.
- Groups are `1rem` apart.
- Page side gutter: `1rem` on phone. On desktop, a `60rem` max-width column.

**The group header** sits inside each sheet:
- A `<button>` row, `44px` tall, with `aria-expanded`.
- It holds, in order:
  1. a chevron;
  2. the label: Source Sans 3, 600, 13.3px, uppercase, `letter-spacing: 0.14em`, `primary`;
  3. the group's pie and number (item F), the composed value's only mark;
  4. the count, 13.3px, `ink-muted`.
- A 4px capsule rule sits directly under the header.

**Folded vs open:** a folded group is the header, then the rule, then `0.75rem` of padding (chevron pointing right). An open group is the header, the rule, then its rows (chevron pointing down).

**Addition 1: nothing sits directly on the boids.**
- The page header gets the same sheet material: the Crimson title plus its capsule rule.
- The bottom word-nav gets the same material too, square at the screen edge, with the bottom safe-area inset added to its padding.
- The design system's rule is that text never sits directly on the flock.

**Addition 2: the phone gutter.** Below 600px, group sheets keep the 8px radius and the 1rem gutter; they don't run edge to edge. Separate groups read better with visible edges.

Agreed reasoning: this list is an app you scan and fold. A frosted, borderless surface per group keeps each group visibly separate at a glance while still following the no-borders, no-shadows rule.

## C. Crimson Pro page titles: agreed

- Page titles "Todos", "Inbox", "Secretary", "Briefing": Crimson Pro 600.
- Size: `33.2px` on phone, `39.8px` from 900px wide.
- Line-height `1.1`, colour `ink`.
- Followed by the 4px capsule rule.
- Vendor `CrimsonPro-200-900.woff2` (latin, variable weight).
- Nothing else uses Crimson Pro.

## D. Colour-only marks (2026-09-26): the ring and the bar, replacing the height fuse

**Superseded by §F (2026-09-28):** the ring and the bar are gone. Kept as the
record of what was decided and why it changed.

**The fuse is the only fulfillment mark on the Todos list** (the owner, 2026-09-25: "there's both the pie and the fuse on the main page, should only be fuse"). Rows, group headers and a folded group's peek line carry no pie, in every grouping and in the desktop list beside the pane. A node the tab cannot yet draw keeps a blank of the mark's size: a dashed `problem` outline (a circle, since the mark it stands in for is a ring) for a group it cannot compose, a dashed `grid` circle for a line the core has not sampled yet.

**State thresholds are unchanged**: the app's existing Problem/Watch/Fine classification (`fulfillment.WATCH = 0.5`, `fulfillment.FINE = 0.7`) — this redesign only changes how a state is DRAWN, never where one starts. State colours are §A's, unchanged from the fuse: `problem` `#e55900`, `watch` `#a44554`, `fine` `#379775`, both themes.

**A1, the ring** — one todo's line, a group header's composed value, and the folded group's peek — replaces the line-and-area fuse with a mark that carries no height, only colour. It shows the next 72 hours, sampled hourly (72 samples) and merged into runs of one state (`fuse.stateRuns`), each drawn as one SVG arc (`fuse.ring`):
- A thin ring: 16px outer diameter, 2.5px stroke, in rows; 20px outer diameter, 3px stroke, in group headers (`fuse.ROW`/`fuse.GROUP`).
- Now is at 12 o'clock, time runs clockwise. Each run is one arc in its state's colour; adjacent runs meet edge to edge, with a hard switch and no blending.
- A 6° seam is left at 12 o'clock, centred there, where now meets +72h — the ring is drawn as a 354° sweep, never a closed loop.
- `role="img"` plus an `aria-label`, for example "Fulfillment 62% now, watch; problem in about 30 hours" (`fuse.label`): the value as a rounded percent, the state now, and the next state the value would cross into within the window, named and timed the way the app already timed a threshold crossing.
- No axes, frame or dot.

**B1, the bar** — the todo page's own thirty-day mark, replacing the uPlot line chart and the header's 48px pie (the header keeps the number, in `ink`; state colours are never text):
- Full content width × 12px, rounded ends (radius 6px), the past fifteen days on the left at 35% opacity and the next fifteen on the right at full opacity — one CSS flex row of state-coloured segments (`fusechart.view`), no SVG.
- Hard state colours, sampled at 120 samples per half (`fuse.stateRuns` again, the same mechanism the ring uses, over each half's own domain).
- A 2px `ink` tick at the middle (now), 20px tall, so it stands slightly clear of the bar above and below.
- Under it, five small `ink-muted` labels (13.3px → 11.1px `t-tiny`, the app's existing date format, `format.day`): the start date, the date seven days back, "now" (in `ink`), the date seven days ahead, and the end date.
- `role="img"` on the bar itself, with the same `aria-label` shape as the ring's (`fuse.label`, over the ahead half).

**Both marks are colour-only, sampled from the wasm core's own p = −4 power-mean composition** — nothing here changes what a group's value is, only how a value becomes a colour. `fuse.valueAt` reads the core's knots the way the line fuse always did (linear between two, flat past either end); `fuse.stateRuns` is the one function that turns a value-reading function into runs of state, shared by both marks and by nothing else.

**Dark mode:** the same state colours (`design/tokens.json`, both themes identical), `grid` `#312d2c`. No motion.

Settled by the owner 2026-09-25: no now-dot, no diamond, no motion. Settled 2026-09-26: colour only, no height on the list, and a plain flex bar (not a chart) for the thirty-day window.

## F. Continuous colour, a small pie, one line (2026-09-28): replaces §D

The owner reviewed the options on real data and chose this; it supersedes §D
(the 72-hour ring and the discrete thirty-day bar) and §A's three state
colours. His reasons:

- **The colour is continuous.** A value is a point on one gradient, never one
  of three hard state colours: three buckets hid the difference between 0.51
  and 0.69, and a hard switch at a threshold drew a jump the value never made.
- **The pie is back, and small.** A pie beside the number reads at a glance,
  where a ring of hourly runs had to be decoded; and a row needs only today's
  value, which the fold already has. Sampling every node's next 72 hours was
  why the list showed dashed placeholders for twenty seconds or more on a cold
  load.
- **The thirty days are one line.** Height says how much, colour says how
  bad, and the two agree at every point; a bar of runs said only which bucket.

**Superseded 2026-10-04:** the colour is now the owner's design system's
scale (typst-design's `fulfillment`, five OKLCH stops from crimson to teal),
and every role is a sample of its ramps, printed from `tokens.typ`
(the `adopt-typst-design-chmhdz` report). As decided then:

**The colour** is Typst's `gradient.linear(color.oklch(color.red),
color.oklch(color.green), space: oklch).sample(v)` for a fulfillment `v` in
[0, 1]. Sampled by Typst it is linear in each OKLCH channel — L 65.95% →
73.96%, C 0.227 → 0.220, hue 28.44° → 144.29°, increasing, through orange and
yellow — so `colour(v) = oklch(65.95 + 8.01·v %, 0.227 − 0.007·v, 28.44 +
115.85·v)`. It is defined once, in `design/tokens.json`'s `fulfillment`, and
`scripts/gen-tokens.mjs` generates it into `web/static/tokens.css`
(`--fulfillment-from`/`--fulfillment-to`), `tokens.typ` (the gradient
`fulfillment`, which `utility.typ` samples for a todo's colour, so the
document matches the app) and `web/src/tokens.ts` (the two ends
`fulfillment.colour` interpolates). The same in both themes.

**A row, a group header and a folded group's peek line** (`row.markOf`):
- A 14px pie (`fulfillment.pie`) beside the number, which is in `ink`. Its
  filled share, from 12 o'clock clockwise, is the value, in `colour(v)`, on a
  faint track disc of the same colour, with a 1px outline in it.
- A group header's pie shows its composed value (the core's p = −4 power
  mean, unchanged).
- A node with no value at all keeps a blank of the pie's size: a dashed circle
  in the gradient's red end.

**A todo's thirty days** (`curve.line`; 15 days back and 15 ahead, the window
the todo page always had):
- One thick line, 4px on screen whatever the width (`vector-effect:
  non-scaling-stroke`), round caps and joins, coloured at every point by
  `colour` of its own value: a vertical gradient on the stroke keyed to the y
  axis, 21 stops computed in OKLCH.
- Inside a clear frame from 0% to 100%, light 0 / 50 / 100 labels and a faint
  dashed line at 50%.
- The past half at 35% opacity. A thin `ink` line at now, and a dot at now's
  value in `colour(v)` with a ring in the sheet colour.
- The five date labels as before — the start, seven days back, "now", seven
  days ahead, the end — each the reader's locale's short day ("Sep 13"), and
  the two a week either side of now drop out where the five would not fit
  without touching (the main session's review, 2026-09-28).
- It samples only this todo, from the core's knots (`thirty.ts`), and draws
  them joined by straight lines, the core's own reading between two knots.

## G. A fraction is a pie (2026-10-24)

Every fraction the web app shows is the pie of §F, drawn by one component
(`web/src/view/pie.ts`) to typst-design's geometry, so a person learns one
mark. Its colour says what kind of fraction it is: the fulfillment scale only
for a judgement (a todo's fulfillment, an allowance left), and the one neutral
accent (`accent` in `tokens.typ`, the blue ramp) for everything else: a sync,
a fetch, an import, a share of a whole, the loading mark. The sync line and
the usage page's bars became pies.

## E. Everything else: agreed (as in `design-review.md`)

**Tokens (light / dark):**

| Token | Light | Dark |
|---|---|---|
| ground | `#faf8f5` | `#100c0c` |
| sheet | `#ffffffbf` | `#1b1718bf` |
| ink | `#2b2b2b` | `#e8e3e3` |
| ink-muted | `#575757` | `#aaa3a2` |
| primary | `#6a001a` | `#e4aeaf` |
| primary-soft | `#922840` | `#d39093` |
| rule | `#f9d4d3` | `#58262f` |
| hover / selected tint | `#faf2f3` | `#400919` |
| grid | `#e7e3e3` | `#312d2c` |
| mark-fill (diamond) | `#faf2f3` | same in both |
| mark-line (diamond) | `#e0b8bd` | same in both |

- Focus: 2px solid `primary`, 2px offset.
- `<meta name="theme-color">`: `#6a001a` for light, `#1b1718` for dark.

**Replaced values:**
- `#701112` → `#6a001a`
- `#8d4141` → `#922840`
- `#f0dcde` → `#f9d4d3`
- `#000000a8` → `#575757`
- `#f2f2f2` → `grid`
- the hand-derived dark theme → the dark column above

**Type, on the ×1.2 scale:**
- body: 16px, line-height 1.55;
- small text and labels: 13.3px, labels tracked 0.14em;
- tiny text: 11.1px;
- page title: 33.2px / 39.8px (item C).

**Diamond** (the CV geometry), identical in both themes:

```css
.diamond {
  display: inline-block;
  box-sizing: border-box;
  width: 0.5em;
  height: 0.5em;
  border: 0.1em solid #e0b8bd;
  border-radius: 0.15em;
  background: #faf2f3;
  transform: rotate(45deg);
  margin: 0 0.4em;
  vertical-align: 0.1em;
}
```

**Nav current-page marker:** the same diamond, at the nav text size, before the current word. That word is `primary` at 600, with `aria-current="page"`.

**Links:** underlined in running text; navigation underlines only on hover.
