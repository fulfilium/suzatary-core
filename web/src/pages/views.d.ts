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
  reply(json: string, text: Uint8Array): Uint8Array;
  /** undefined when no handler took the event. */
  event(id: number, on: number, words: string): Uint8Array | undefined;
  /** The current view drawn from nothing, for a new painter. */
  fresh(): Uint8Array;
  /** A JSON array, since last asked. */
  effects(): string;
  /** A JSON array of `SubPatch`es, since last asked: applied before the effects of the same steps. */
  subs(): string;
  free(): void;
  /** `{"flags": …, "inputs": […]}` (`Hosted::rehearsal`), or undefined for a page that has none. */
  static rehearsal(): string | undefined;
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
export class ConversationsPage extends Page {}
export class DraftPage extends Page {}
export class NeedsYouPage extends Page {}
/** A todo's page (`app/pages/src/todo.rs`). */
export class TodoPage extends Page {}
/** The list (`app/pages/src/todos.rs`). */
export class TodosPage extends Page {}
/** The list drawn by Typst (`app/pages/src/typstlist.rs`). */
export class TypstPage extends Page {}

/** The tiles of the app's pages (`app/pages/src/desk.rs`): the tree, its acts, its address, and its drawing. */
export class Desk {
  /** Throws where the bar's destinations are not a well-formed home. */
  constructor();
  /** An address, read unless it is the desk's own print. */
  go(address: string): void;
  /** The tree's print, `#` first. */
  address(): string;
  /** The focused workspace's home, `#` first. */
  workspace(): string;
  /** The page `href` names, opened from the tile `key`. */
  open(key: number, href: string): void;
  /** JSON of `Dock`, the shell's say on the secretary beside the pages; throws on a say that does not read. */
  dock(dock: string): void;
  /** The divider between the tiles `before` and `after` to the shares of weights `a` and `b`, 1 to 3. */
  resize(before: number, after: number, a: number, b: number): void;
  /** JSON of `Drawing`, for a viewport in pixels at the 16px root. */
  drawing(w: number, h: number): string;
  free(): void;
}
