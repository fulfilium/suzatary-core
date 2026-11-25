/**
 * The node surface the browser tests use, declared since there is no npm and so no `@types/node`; `WebSocket`, `URL`
 * and `setTimeout` come from the DOM lib. The modules `web/test/node.d.ts` already declares are augmented, since
 * ambient module declarations merge and a second `test` would be a duplicate identifier.
 */

declare module "node:test" {
  /** Runs after every test in the file, however they ended. */
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
    /** Both `null` while it is still running, and one of them set once node has reaped it, which `cdp.ts` waits on before
     * removing a profile, because `kill` only says the signal was delivered. */
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
  export function execFileSync(
    command: string,
    args: readonly string[],
    options: { readonly encoding: "utf8"; readonly env?: Record<string, string | undefined>; readonly timeout?: number },
  ): string;
  /** `execFileSync` beside the caller: `done` is told how it ended. */
  export function execFile(
    command: string,
    args: readonly string[],
    options: { readonly encoding: "utf8"; readonly timeout?: number },
    done: (error: Error | null, stdout: string, stderr: string) => void,
  ): ChildProcess;
}

declare module "node:fs" {
  export function mkdtempSync(prefix: string): string;
  export function mkdirSync(path: string, options?: { readonly recursive?: boolean }): void;
  export function readFileSync(path: string, encoding: string): string;
  export function readFileSync(path: string): Uint8Array<ArrayBuffer>;
  export function readdirSync(path: string): string[];
  export function writeFileSync(path: string, data: string | Uint8Array, encoding?: string): void;
  export function existsSync(path: string): boolean;
  export function rmSync(path: string, options?: { readonly recursive?: boolean; readonly force?: boolean }): void;
  export function rmdirSync(path: string): void;
  export function copyFileSync(from: string, to: string): void;
  export function renameSync(from: string, to: string): void;
  export function appendFileSync(path: string, data: string): void;
  export function statSync(path: string): { readonly size: number };
}

declare module "node:os" {
  export function availableParallelism(): number;
  export function loadavg(): number[];
  export function tmpdir(): string;
}

declare module "node:path" {
  export function join(...parts: readonly string[]): string;
  export function dirname(path: string): string;
  export function basename(path: string, suffix?: string): string;
  export function normalize(path: string): string;
}

declare const process: {
  readonly env: Record<string, string | undefined>;
  readonly platform: string;
  readonly pid: number;
  readonly argv: readonly string[];
};

declare module "node:url" {
  /** An absolute path as a `file:` URL, the one form `import()` takes for an ES module on disk. */
  export function pathToFileURL(path: string): { readonly href: string };
}
