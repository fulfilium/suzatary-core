// Every ui register and its tier. `Tab`: gone with the tab. `Device`: this browser, across tabs. `Owner`: also follows
// the owner to other devices. The route is kept elsewhere, as App's log (`shell/routelog.ts`).

import ui from "./ui"
import d from "../wire/decoder"
import type { Theme } from "./theme"
import type aside from "../chat/aside"

export namespace registers {
  // Per device: a phone in the dark and a desk in daylight differ.
  export const theme = ui.kind<Theme>({ name: "theme", tier: "Device", initial: "auto", value: d.oneOf({ auto: null, light: null, dark: null }) })
  // "" for the deployment's first chain.
  export const chain = ui.kind<string>({ name: "chain", tier: "Device", initial: "", value: d.str })
  // By page (`chat/aside.ts`).
  export const panel = ui.kind<aside.Presses>({ name: "panel", tier: "Tab", initial: {} })

  // By conversation: the owner's words, so they follow the owner.
  export const draft = ui.kind<string>({ name: "draft", tier: "Owner", initial: "", value: d.str })

  // What a Rust page keeps across its draws (a card's step and words), by the page's key.
  export const page = ui.kind<unknown>({ name: "page", tier: "Tab", initial: null })
  // A part of a Rust page's screen this device keeps across its loads, by key.
  export const kept = ui.kind<unknown>({ name: "kept", tier: "Device", initial: null, value: d.unknown })
  // Whether the shell's line of what this device left out is opened to its names: this tab's, kept as readings move it.
  export const leftout = ui.kind<boolean>({ name: "leftout", tier: "Tab", initial: false })

  // So a new build is announced once across tabs.
  export const announced = ui.kind<string>({ name: "announced", tier: "Device", initial: "", value: d.str })
}
export default registers
