// The merge gate's units: prints `<unit>\t<space-separated paths it does not read>` per unit.
//
//   node scripts/gate-units.mjs crates <metadata.json> <root> <tree> [--base <path>...] [<package>=<path>]...
//       One unit per workspace member, reading its own and its transitive dependencies'
//       directories and declared paths. Every member's Cargo.toml is every unit's input:
//       features unify across the workspace.
//   node scripts/gate-units.mjs files <metafile.json> <tree> <entries> --scope <dir>... [--shared [<entry>=]<path>...] [--base <path>...]
//       One unit per entry point, reading what its imports reach and each --shared path
//       (`<entry>=<path>` for that entry alone).
//
// Any failure, an import under a --base path among them, exits nonzero, and the step runs whole.
import { readFileSync } from "node:fs";
import { relative, dirname } from "node:path";

const [mode, ...args] = process.argv.slice(2);
const under = (path, dir) => path === dir || path.startsWith(dir + "/");
const fail = why => { console.error(`gate-units: ${why}`); process.exit(1); };
const treeOf = file => readFileSync(file, "utf8").split("\n").filter(l => l !== "").map(l => l.slice(l.indexOf("\t") + 1));

// The words before the first flag are under "".
const flags = words => {
  const out = { "": [] };
  let at = "";
  for (const word of words) {
    if (word.startsWith("--")) { at = word.slice(2); out[at] ??= []; } else out[at].push(word);
  }
  return out;
};

const crates = (metadataFile, root, treeFile, rest) => {
  const { "": declared, base = [] } = flags(rest);
  const metadata = JSON.parse(readFileSync(metadataFile, "utf8"));
  const tree = treeOf(treeFile);
  const members = new Map(metadata.packages.map(p => [p.name, relative(root, dirname(p.manifest_path))]));
  const byDir = new Map([...members].map(([name, dir]) => [dir, name]));
  const edges = new Map(metadata.packages.map(p => [p.name,
    p.dependencies.filter(d => d.path !== undefined).map(d => byDir.get(relative(root, d.path))).filter(n => n !== undefined)]));
  const extra = new Map();
  for (const entry of declared) {
    const at = entry.indexOf("=");
    if (at < 1 || !members.has(entry.slice(0, at))) fail(`no workspace member in ${entry}`);
    extra.set(entry.slice(0, at), [...(extra.get(entry.slice(0, at)) ?? []), entry.slice(at + 1)]);
  }
  for (const name of members.keys()) {
    const closure = new Set([name]);
    const todo = [name];
    while (todo.length > 0) for (const next of edges.get(todo.pop()) ?? []) if (!closure.has(next)) { closure.add(next); todo.push(next); }
    const reads = [...closure].flatMap(n => [members.get(n), ...(extra.get(n) ?? [])]);
    const read = path => reads.some(r => under(path, r) || under(r, path));
    const others = [...members.values()].filter(dir => !read(dir)).flatMap(dir => [...new Set(
      tree.filter(p => under(p, dir) && p !== `${dir}/Cargo.toml`).map(p => `${dir}/${p.slice(dir.length + 1).split("/")[0]}`))]);
    console.log(`${name}\t${[...base.filter(p => !read(p)), ...others].join(" ")}`);
  }
};

const files = (metafile, treeFile, entries, rest) => {
  const { scope = [], shared = [], base = [] } = flags(rest);
  const inputs = JSON.parse(readFileSync(metafile, "utf8")).inputs;
  const tree = treeOf(treeFile);
  // The generated wire is outside the tree, so what it imports is read as all of web/src/wire.
  const generated = "web/src/wire/generated";
  const roots = Object.keys(inputs).filter(p => dirname(p) === entries && p.endsWith(".test.ts"));
  if (roots.length === 0) fail(`no entry under ${entries} in ${metafile}`);
  for (const root of roots.sort()) {
    const closure = new Set([root]);
    const todo = [root];
    while (todo.length > 0) {
      for (const { path } of inputs[todo.pop()]?.imports ?? []) {
        if (inputs[path] !== undefined && !closure.has(path)) { closure.add(path); todo.push(path); }
      }
    }
    const skipped = [...closure].find(p => base.some(b => under(p, b)));
    if (skipped !== undefined) fail(`${root} imports ${skipped}, which every unit skips`);
    const own = shared.flatMap(s => {
      const at = s.indexOf("=");
      return at < 0 ? [s] : s.slice(0, at) === root ? [s.slice(at + 1)] : [];
    });
    const reads = [...closure, ...own, ...([...closure].some(p => under(p, generated)) ? ["web/src/wire"] : [])];
    const skips = tree.filter(p => scope.some(s => under(p, s)) && !reads.some(r => under(p, r)));
    console.log(`${root}\t${[...base, ...skips].join(" ")}`);
  }
};

switch (mode) {
  case "crates": crates(args[0], args[1], args[2], args.slice(3)); break;
  case "files": files(args[0], args[1], args[2], args.slice(3)); break;
  default: fail("usage: gate-units.mjs crates <metadata> <root> <tree> [--base <path>...] [<package>=<path>]... | files <metafile> <tree> <entries> --scope <dir>... [--shared [<entry>=]<path>...] [--base <path>...]");
}
