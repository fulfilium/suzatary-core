/**
 * The pages' module (`app/pages`, `scripts/views-wasm.sh`), declared: a build output copied beside the bundle
 * (`build-web.sh`), so `import("./views.js")` resolves here for `tsc` while esbuild keeps the specifier external. Each
 * class is one page as `viewtree::export!` makes it.
 */

/** Instantiate the module once per tab, from a response it streams. */
export default function init(init: { module_or_path: Response | BufferSource }): Promise<unknown>;

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
