// A disabled act and a neutral pie are visible in both colour sets: each outline holds WCAG's 3:1 for a graphical edge
// against what it sits on.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

const style = readFileSync("web/static/style.css", "utf8")
const tokens = readFileSync("web/static/tokens.css", "utf8")

const block = (css: string, selector: string): string => {
  const at = css.indexOf(`${selector} {`)
  assert.ok(at >= 0, `${selector} is styled`)
  return css.slice(at, css.indexOf("}", at))
}

// The colour roles as one set declares them: the light set is the first `:root` block naming the sheet, the dark one is
// under `data-theme="dark"`.
const roles = (set: "light" | "dark"): Map<string, string> => {
  const light = tokens.split(":root {").find(b => b.includes("--sheet-solid")) ?? ""
  const text = set === "light" ? light.slice(0, light.indexOf("}")) : block(tokens, ':root[data-theme="dark"]')
  return new Map([...text.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{6})/g)].map(m => [m[1] ?? "", m[2] ?? ""]))
}

const luminance = (hex: string): number => {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255).map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0)
}

const contrast = (a: string, b: string): number => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05)
}

test("a disabled act is not faded, and its outline and words stand out from the card in light and dark", () => {
  const rule = block(style, ".act[disabled]")
  assert.ok(!rule.includes("opacity"), "fading takes the outline below what an eye sees on a dark card")
  const role = (property: string): string => {
    const m = new RegExp(`${property}:\\s*var\\(--([a-z0-9-]+)\\)`).exec(rule)
    assert.ok(m?.[1] !== undefined, `${property} is a colour role`)
    return m[1]
  }
  for (const set of ["light", "dark"] as const) {
    const r = roles(set)
    const card = r.get("sheet-solid") ?? ""
    for (const property of ["border-color", "color"]) {
      const colour = r.get(role(property)) ?? ""
      assert.ok(contrast(colour, card) >= 3, `${set}: ${property} ${colour} on ${card} is ${contrast(colour, card).toFixed(2)}:1`)
    }
  }
})

test("a neutral pie wears the accent, whose outline stands out from the ground and the card in light and dark", () => {
  assert.match(block(style, ".pieneutral"), /color:\s*var\(--accent\)/)
  for (const set of ["light", "dark"] as const) {
    const r = roles(set)
    const accent = r.get("accent") ?? ""
    for (const under of ["ground", "sheet-solid"]) {
      const behind = r.get(under) ?? ""
      assert.ok(contrast(accent, behind) >= 3, `${set}: the accent ${accent} on ${under} ${behind} is ${contrast(accent, behind).toFixed(2)}:1`)
    }
  }
})
