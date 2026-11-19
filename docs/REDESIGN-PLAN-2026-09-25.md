# The web app in the bmabsout hand — 2026-09-25

The owner approved the canvas, a private claude.ai artifact ("build this").
The design system is his `bmabsout`, another. Everything here is
settled; the tree behind the Todos list is `app/base/src/tree.rs`.
TypeScript follows docs/TS-STYLE.md.

## Tokens

The light theme is the system's "screen" set.

| role | token | value |
| --- | --- | --- |
| ground | `screen` | `oklch(98% 0.005 80)` |
| card | `glass` | `#ffffffbf`, 2px `shade-line` border, 8px radius, no shadow |
| text | `ink-soft` / `ink-muted` | `#2b2b2b` / `#000000a8` |
| accent, links, labels | `primary` | `#701112` |
| soft accent | `primary-soft`, `shade`, `shade-line`, `rule-dots` | `#8d4141`, `#faf2f3`, `#e0b8bd`, `#f0dcde` |
| problem | `orange-50` | `oklch(50% 0.292 43)`, wash `oklch(99.1% 0.033 51.2)` |
| watch | `main-45` | `oklch(45% 0.141 11.8)`, wash `oklch(98.5% 0.009 15.6)` |
| fine | `teal-55` | `oklch(55% 0.104 166)`, wash `oklch(97.8% 0.017 170)` |
| empty part of a pie | `zebra` | `#f2f2f2` |
| callout (explanations) | `blue-35` on `blue-wash`, outline `blue-line` | |

Dark theme (the system has none; sample the same gradients at mirrored
lightness):
- Ground is `oklch(17% 0.012 20)` and cards are `oklch(21% 0.014 20)`.
- Text is `oklch(93% 0.01 80)`; muted text is `oklch(93% 0.01 80 / 0.66)`.
- Primary is `oklch(78% 0.09 18)`.
- The three states are `oklch(72% 0.19 43)`, `oklch(74% 0.1 12)` and
  `oklch(72% 0.1 166)`.
- `zebra` is `oklch(30% 0.01 20)`.

Type:
- Source Serif 4 sets everything that is read.
- Source Sans 3 is only for short labels: UPPERCASE, 600 or 700 weight,
  tracked 0.1 to 0.15em.
- Both faces ship as woff2 under web/static (OFL). No request goes to Google.

## Marks

- **The capsule rule.** A 4px stroke, round caps, dash 4 gap 8 (SVG, never a
  CSS dashed border). It goes under the page header, between a card's
  headers and its body, and as the outline of a callout.
- **The diamond.** A 0.4em square turned 45°, in `shade` with a `shade-line`
  edge. It separates inline items.
- **Buttons are capsules.** Primary is filled `primary` with white text;
  secondary is a 2px `shade-line` border with `primary` text.
- **Links.** An inline link in running text is `primary` and underlined.
  Navigation is not underlined: tabs, back crumbs and buttons.
- **Navigation is words, not icons.** On a phone it is a bottom bar: Todos,
  Inbox (with its count), Secretary, Briefing. The current page gets a small
  diamond above it.

## Fulfillment

It is always a pie filled clockwise to the value, with the empty part in
`zebra`. Rows use 14px, groups 20px, and the todo page 48px. Its colour is the
value's state:

| state | value | colour |
| --- | --- | --- |
| Problem | below 0.5 | orange-50 |
| Watch | 0.5 to below 0.7 | main-45 |
| Fine | 0.7 and up | teal-55 |

The number itself appears only on the todo page and in "Why".

## The Todos list

- **One line per todo:** pie, title (ellipsis), and the subtodo count on a
  parent. Tapping a row opens it in place: its text, then Open, Done and Ask.
- **Groups** are glass cards. The header shows the composed pie, the name in
  labels style, and "N open". The default group-by is category. The others are
  Urgency (sections Problem, Watch, Fine) and Flat.
- **Urgency sets the order.** Groups sort by composed value, and rows sort
  within every level.
- **Folding follows need.** A fold nobody has touched is open exactly when its
  value is not Fine. Folds the viewer sets live in the page's own state, per
  device, never on the chain.
- **A folded group keeps a peek:** one italic line with the most urgent todo
  inside it and that todo's 10px pie.
- **The header reads "N need attention ◆ M wait on you".** N counts todos in
  Problem. The second phrase is an inline link to the Inbox.
