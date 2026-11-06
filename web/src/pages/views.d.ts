// The pages' wasm module (`app/pages`), copied beside the bundle by `build-web.sh`: declared so `tsc` resolves
// `import("./views.js")` while esbuild keeps it external.

/** Once per tab. */
export default function init(init: { module_or_path: Response | BufferSource | WebAssembly.Module }): Promise<unknown>;

/** A page as `viewtree::export!` makes it. */
declare class Page {
  /** Throws when the JSON does not read as the page's flags. */
  constructor(flags: string);
  /** Once. */
  first(): Uint8Array;
  input(json: string): Uint8Array;
  /** undefined when no handler took the event. */
  event(id: number, on: number, words: string): Uint8Array | undefined;
  /** The current view drawn from nothing, for a new painter. */
  fresh(): Uint8Array;
  /** A JSON array, since last asked. */
  effects(): string;
  free(): void;
  /** Runs the page's rehearsal (`Hosted::rehearsal`) and drops it; throws when a step of it fails. */
  static rehearse(): void;
}

export class MailPage extends Page {}
export class MessagePage extends Page {}
export class ThreadPage extends Page {}
export class MemoryPage extends Page {}
export class AboutPage extends Page {}
export class CalendarPage extends Page {}
export class BriefingsPage extends Page {}
export class BriefingPage extends Page {}

export class SettingsPage extends Page {}
export class InboxPage extends Page {}
export class SecretaryPage extends Page {}
export class ConversationPage extends Page {}
export class DraftPage extends Page {}
export class NeedsYouPage extends Page {}
/** A todo's page (`app/pages/src/todo.rs`). */
export class TodoPage extends Page {}
/** The list (`app/pages/src/todos.rs`). */
export class TodosPage extends Page {}
/** The list drawn by Typst (`app/pages/src/typstlist.rs`). */
export class TypstPage extends Page {}
