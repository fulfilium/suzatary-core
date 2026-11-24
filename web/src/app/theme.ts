// Light or dark: `auto` follows the machine and writes nothing, `light`/`dark` override it. Three
// states, because a two-state toggle would pin a reader who never touched it to the first answer.

import ui from "./ui";
import registers from "./registers";

export type Theme = "auto" | "light" | "dark";

export const THEMES: readonly Theme[] = ["auto", "light", "dark"];

/** The root's `data-theme`, absent for `auto` so the stylesheet's `:not([data-theme="light"])` guard holds. */
export function attributeFor(theme: Theme): string | null {
  return theme === "auto" ? null : theme;
}

export function labelFor(theme: Theme): string {
  switch (theme) {
    case "auto":
      return "System";
    case "light":
      return "Light";
    case "dark":
      return "Dark";
  }
}

const chosen = ui.cell(registers.theme);

export const theme = (): Theme => chosen.get();

function paint(): void {
  const value = attributeFor(theme());
  if (value === null) document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", value);
}

/** Called once before the first route, so a page never paints light and flashes dark; another tab's choice repaints. */
export function restoreTheme(): void {
  paint();
  chosen.hear(paint);
}

export const setTheme = (next: Theme): void => chosen.set(next);

export const hearTheme = (f: () => void): (() => void) => chosen.hear(f);
