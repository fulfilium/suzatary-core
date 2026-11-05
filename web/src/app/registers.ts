// Every ui register and its tier. `Tab`: gone with the tab. `Device`: this browser, across tabs. `Owner`: also follows
// the owner to other devices. The route is kept elsewhere, as App's log (`shell/routelog.ts`).

import ui from "./ui"
import d from "../wire/decoder"
import type { Theme } from "./theme"
import type grouping from "../todo/grouping"
import type row from "../todo/row"
import type aside from "../chat/aside"

export namespace registers {
  // A fold whose value is not a flag is dropped, never the whole set.
  const flags: d.Decoder<row.Folds> = d.map(d.rec(d.unknown), o =>
    Object.fromEntries(Object.entries(o).filter((kv): kv is [string, boolean] => typeof kv[1] === "boolean")),
  )

  // Per device: a phone in the dark and a desk in daylight differ.
  export const theme = ui.kind<Theme>({ name: "theme", tier: "Device", initial: "auto", value: d.oneOf({ auto: null, light: null, dark: null }) })
  // "" for the deployment's first chain.
  export const chain = ui.kind<string>({ name: "chain", tier: "Device", initial: "", value: d.str })
  // By page (`chat/aside.ts`).
  export const panel = ui.kind<aside.Presses>({ name: "panel", tier: "Tab", initial: {} })

  // Per device: a phone's list is read differently.
  export const filter = ui.kind<string>({ name: "todos.filter", tier: "Device", initial: "", value: d.str })
  export const by = ui.kind<grouping.Grouping>({ name: "todos.by", tier: "Device", initial: "category", value: d.oneOf({ category: null, urgency: null, flat: null }) })
  export const folds = ui.kind<row.Folds>({
    name: "todos.folds",
    tier: "Device",
    initial: {},
    value: flags,
  })
  // By chain, so back to the list finds it open.
  export const opened = ui.kind<string>({ name: "todos.opened", tier: "Tab", initial: "" })
  // The last todo's category, so the next starts there.
  export const category = ui.kind<string>({ name: "todos.category", tier: "Device", initial: "", value: d.str })
  // By chain; drawn from here, never read back from the page.
  export const sheet = ui.kind<boolean>({ name: "todos.sheet", tier: "Tab", initial: false })

  // By conversation: the owner's words, so they follow the owner.
  export const draft = ui.kind<string>({ name: "draft", tier: "Owner", initial: "", value: d.str })

  // What a Rust page keeps across its draws (a card's step and words), by the page's key.
  export const page = ui.kind<unknown>({ name: "page", tier: "Tab", initial: null })
  // By todo and part.
  export const told = ui.kind<boolean>({ name: "todo.told", tier: "Tab", initial: false })
  export const leftout = ui.kind<boolean>({ name: "leftout", tier: "Tab", initial: false })

  // So a new build is announced once across tabs.
  export const announced = ui.kind<string>({ name: "announced", tier: "Device", initial: "", value: d.str })
}
export default registers
