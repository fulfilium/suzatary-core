// `__DEV__` is an esbuild define, always a literal before anything runs.

declare const __DEV__: boolean;

// A build's chunks, named by their content, handed to the worker (`sw.ts`) as it is bundled.
declare const __CHUNKS__: readonly string[];

// The chunks `app.js` imports as it loads, handed to the loader (`boot.ts`) so it asks for them beside `app.js`.
declare const __PRELOAD__: readonly string[];

// What this build names each file named by its bytes (`shell.Manifest`), handed to every bundle that asks for one.
declare const __CONTENT__: import("./app/shell").default.Manifest;
