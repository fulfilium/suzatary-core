/**
 * The node surface the browser tests use, declared — the same bargain
 * `web/test/node.d.ts` makes, one directory over.
 *
 * No `@types/node`, because there is no npm here at all: the toolchain is
 * pinned in flake.nix, and twenty lines we can read are a better dependency
 * than a package tree we cannot. `tsc -p web` typechecks this directory
 * against these, so a call to something undeclared fails the build.
 *
 * `WebSocket`, `URL` and `setTimeout` are NOT here: the DOM lib already
 * declares them and node 24 implements them, which is the whole reason the
 * DevTools client below needs no library.
 *
 * The two modules `web/test/node.d.ts` already declares are augmented rather
 * than redeclared — ambient module declarations merge, and a second copy of
 * `test` would be a duplicate identifier.
 */

declare module "node:test" {
  /** Runs after every test in the file, however they ended: where a browser
   * gets closed. */
  export function after(fn: () => void | Promise<void>): void;
}

declare module "node:child_process" {
  interface Readable {
    on(event: "data", listener: (chunk: { toString: () => string }) => void): void;
    setEncoding(encoding: string): void;
  }
  export interface ChildProcess {
    readonly pid: number | undefined;
    readonly stderr: Readable | null;
    readonly stdout: Readable | null;
    /** Both `null` while it is still running, and one of them set once node has
     * REAPED it — which is the fact `cdp.ts` waits on before removing a
     * profile, because `kill` only says the signal was delivered. */
    readonly exitCode: number | null;
    readonly signalCode: string | null;
    kill(signal?: string): boolean;
    on(event: "exit", listener: (code: number | null, signal: string | null) => void): void;
    on(event: "error", listener: (error: Error) => void): void;
    once(event: "exit", listener: (code: number | null, signal: string | null) => void): void;
  }
  export function spawn(
    command: string,
    args: readonly string[],
    options?: { readonly stdio?: readonly string[]; readonly env?: Record<string, string | undefined> },
  ): ChildProcess;
  /** RUN A PROGRAM AND WAIT, for the one scene that has to (§10.6): after a
   * rejection, "does the same writer producing the same bytes get them back"
   * is a question only running the writer answers, and a browser cannot. */
  export function execFileSync(
    command: string,
    args: readonly string[],
    options: { readonly encoding: "utf8"; readonly env?: Record<string, string | undefined> },
  ): string;
}

declare module "node:fs" {
  export function mkdtempSync(prefix: string): string;
  export function mkdirSync(path: string, options?: { readonly recursive?: boolean }): void;
  export function readFileSync(path: string, encoding: string): string;
  /** A file's bytes: the tab's typst-wasm module, which `web/test/typstview.test.ts` instantiates. */
  export function readFileSync(path: string): Uint8Array<ArrayBuffer>;
  /** The store's objects, by name. `web/test/wasm.test.ts` reads
   * `events/objects/` directly since the live chain's vectors stopped carrying
   * a copy of those bytes (docs/ARCHITECTURE.md §8.8). */
  export function readdirSync(path: string): string[];
  export function writeFileSync(path: string, data: string, encoding?: string): void;
  export function existsSync(path: string): boolean;
  export function rmSync(path: string, options?: { readonly recursive?: boolean; readonly force?: boolean }): void;
  /** A bundle's file set aside and put back: `web/e2e/shell.test.ts` deploys a build whose files differ. */
  export function copyFileSync(from: string, to: string): void;
  export function renameSync(from: string, to: string): void;
}

declare module "node:os" {
  export function availableParallelism(): number;
  export function loadavg(): number[];
  export function tmpdir(): string;
}

declare module "node:path" {
  export function join(...parts: readonly string[]): string;
  export function dirname(path: string): string;
}

/** Only what the harness reads. */
declare const process: {
  readonly env: Record<string, string | undefined>;
  readonly platform: string;
};

declare module "node:url" {
  /** An absolute path as a `file:` URL, the one form `import()` takes for an ES module on disk. */
  export function pathToFileURL(path: string): { readonly href: string };
}
