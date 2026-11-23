/**
 * `node:test` and `node:assert/strict`, declared — the minimal subset the
 * tests call.
 *
 * No `@types/node`, for the same reason there is no npm at all here: the
 * toolchain is pinned in flake.nix, and a declaration we can read in twenty
 * lines is a better dependency than one we cannot. `tsc -p web` typechecks the
 * tests against this, so a test that calls something not declared below fails
 * the build rather than at runtime.
 */

declare module "node:test" {
  /** A test's own context: its clock, mocked, for a law about time that must not wait for it. */
  export interface TestContext {
    readonly mock: { readonly timers: { enable(options: { apis: readonly ("setTimeout" | "setInterval")[] }): void; tick(ms: number): void } };
  }
  export function test(name: string, fn: (t: TestContext) => void | Promise<void>): void;
  export function test(name: string, options: { skip: string | false }, fn: () => void | Promise<void>): void;
  export function describe(name: string, fn: () => void): void;
  export function it(name: string, fn: () => void | Promise<void>): void;
}

declare module "node:module" {
  /** The one door from an ES module to a CommonJS one. `suzatary-wasm`'s
   * `--target nodejs` glue is CommonJS and reads its `.wasm` off disk, so this
   * is how `wasm.test.ts` loads the same module a browser loads. */
  export function createRequire(from: string): (id: string) => unknown;
}

declare module "node:assert/strict" {
  interface Assertions {
    (value: unknown, message?: string): void;
    ok(value: unknown, message?: string): asserts value;
    equal(actual: unknown, expected: unknown, message?: string): void;
    notEqual(actual: unknown, expected: unknown, message?: string): void;
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    notDeepEqual(actual: unknown, expected: unknown, message?: string): void;
    throws(fn: () => unknown, message?: string | RegExp): void;
    rejects(promise: Promise<unknown>, message?: string | RegExp): Promise<void>;
    match(actual: string, expected: RegExp, message?: string): void;
    fail(message?: string): never;
  }
  const assert: Assertions;
  export default assert;
}

declare module "node:crypto" {
  /** Enough of `createHash` to rehash a sealed object's literal the way the
   * store names it: sha256 over the UTF-8 bytes, hex out. */
  export function createHash(algorithm: string): {
    update(data: string, encoding: string): { digest(encoding: string): string };
  };
  /** Enough of Ed25519 to sign an object as a device key does: a key pair, its public half as SPKI DER (the raw key is
   * its last 32 bytes), and a signature of bytes. */
  export interface KeyObject {
    export(options: { format: "der"; type: "spki" }): Uint8Array;
  }
  export function generateKeyPairSync(type: "ed25519"): { publicKey: KeyObject; privateKey: KeyObject };
  export function sign(algorithm: null, data: Uint8Array, key: KeyObject): Uint8Array;
}
