/**
 * The pages' module (`app/pages`, `nix build .#suzatary-views-wasm`), declared: a build output copied beside the bundle
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
  /** The current view drawn from nothing, as a new painter draws it. */
  fresh(): Uint8Array;
  /** The effects' JSON array, since last asked. */
  effects(): string;
  free(): void;
}

/** Mail, a message, a thread and Settings (`app/pages/src/{mail,message,thread,settings}.rs`), each speaking the runtime's sums. */
export class MailPage extends Page {}
export class MessagePage extends Page {}
export class ThreadPage extends Page {}
/** Memory, About, the calendar, the briefings and a briefing (`app/pages/src/{memory,about,calendar,briefings,briefing}.rs`). */
export class MemoryPage extends Page {}
export class AboutPage extends Page {}
export class CalendarPage extends Page {}
export class BriefingsPage extends Page {}
export class BriefingPage extends Page {}

export class SettingsPage extends Page {}
