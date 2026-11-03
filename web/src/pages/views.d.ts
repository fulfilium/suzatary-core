/**
 * The pages' module (`app/pages`, `scripts/views-wasm.sh`), declared: a build output copied beside the bundle
 * (`build-web.sh`), so `import("./views.js")` resolves here for `tsc` while esbuild keeps the specifier external. Each
 * class is one page as `viewtree::export!` makes it.
 */

/** Instantiate the module once per tab, from a response it streams. */
export default function init(init: { module_or_path: Response | BufferSource }): Promise<unknown>;

/** A page as `viewtree::export!` makes it: started from its flags' JSON, stepped by inputs and events. */
declare class Page {
  /** Started from the page's flags as JSON; throws when they do not read as its flags. */
  constructor(flags: string);
  /** The first paint's patches, once. */
  first(): Uint8Array;
  /** An input's JSON stepped: its patches. */
  input(json: string): Uint8Array;
  /** An event the painter reported: its patches. */
  event(id: number, on: number, words: string): Uint8Array;
  /** The effects' JSON array, since last asked. */
  effects(): string;
  free(): void;
}

/** Mail, a message and a thread (`app/pages/src/{mail,message,thread}.rs`), each speaking the runtime's sums. */
export class MailPage extends Page {}
export class MessagePage extends Page {}
export class ThreadPage extends Page {}
/** Memory, About, the calendar, the briefings and a briefing (`app/pages/src/{memory,about,calendar,briefings,briefing}.rs`). */
export class MemoryPage extends Page {}
export class AboutPage extends Page {}
export class CalendarPage extends Page {}
export class BriefingsPage extends Page {}
export class BriefingPage extends Page {}

export class SettingsPage {
  /** Started from `SettingsShown`'s JSON; throws when it does not read as one. */
  constructor(flags: string);
  /** The first paint's patches, once. */
  first(): Uint8Array;
  /** `SettingsInput`'s JSON stepped: its patches. */
  input(json: string): Uint8Array;
  /** An event the painter reported: its patches. */
  event(id: number, on: number, words: string): Uint8Array;
  /** `SettingsEffect[]`'s JSON, since last asked. */
  effects(): string;
  free(): void;
}
