// The merge gate's units: prints `<unit>\t<space-separated paths it does not read>` per unit.
//
//   node scripts/gate-units.mjs crates <metadata.json> <root> <tree> [--base <path>...] [<package>=<path>]...
//       One unit per workspace member, reading its own and its transitive dependencies'
//       directories and declared paths. Every member's Cargo.toml is every unit's input:
//       features unify across the workspace.
//   node scripts/gate-units.mjs files <metafile.json> <tree> <entries> --scope <dir>... [--shared [<entry>=]<path>...] [--base <path>...]
//       One unit per entry point, reading what its imports reach and each --shared path
//       (`<entry>=<path>` for that entry alone).
//   node scripts/gate-units.mjs scenes <metafile.json> <tree> <entries> --app <app-metafile> --static <metafile> [--later <path>...] (and files' flags)
//       As `files`, each scene also skipping what a page runs only once it fetches it: the
//       inputs of the app's dynamically loaded chunks, and the --later paths.
//   node scripts/gate-units.mjs narrow <skips-file> <reads-file>
//       The skips a scene keeps once it fetched the paths <reads-file> names.
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

const units = (metafile, treeFile, entries, rest) => {
  const { scope = [], shared = [], base = [] } = flags(rest);
  const inputs = JSON.parse(readFileSync(metafile, "utf8")).inputs;
  const tree = treeOf(treeFile);
  // The generated wire is outside the tree, so what it imports is read as all of web/src/wire.
  const generated = "web/src/wire/generated";
  const roots = Object.keys(inputs).filter(p => dirname(p) === entries && p.endsWith(".test.ts"));
  if (roots.length === 0) fail(`no entry under ${entries} in ${metafile}`);
  return roots.sort().map(root => {
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
    return { root, closure, skips: [...base, ...skips] };
  });
};

const files = (metafile, treeFile, entries, rest) => {
  for (const { root, skips } of units(metafile, treeFile, entries, rest)) console.log(`${root}\t${skips.join(" ")}`);
};

// A page runs a dynamically loaded chunk only once it fetched it, so its inputs, but those the
// --static bundles read, are skipped until `narrow` names them.
const scenes = (metafile, treeFile, entries, rest) => {
  const { app = [], static: fixed = [], later = [] } = flags(rest);
  if (app.length !== 1 || fixed.length !== 1) fail("scenes takes --app <metafile> and --static <metafile>");
  const outputs = JSON.parse(readFileSync(app[0], "utf8")).outputs;
  const always = new Set(Object.keys(JSON.parse(readFileSync(fixed[0], "utf8")).inputs));
  const main = Object.keys(outputs).find(p => /(^|\/)app\.js$/.test(p));
  if (main === undefined) fail(`no app.js among the outputs of ${app[0]}`);
  const loaded = new Set([main]);
  const todo = [main];
  while (todo.length > 0) {
    for (const { path, kind } of outputs[todo.pop()].imports) {
      if (kind === "import-statement" && outputs[path] !== undefined && !loaded.has(path)) { loaded.add(path); todo.push(path); }
    }
  }
  const dynamic = Object.entries(outputs).filter(([path]) => path.endsWith(".js") && !loaded.has(path))
    .flatMap(([, out]) => Object.keys(out.inputs)).filter(path => !always.has(path));
  const tree = treeOf(treeFile);
  const fetched = tree.filter(p => [...dynamic, ...later].some(d => under(p, d)));
  for (const { root, closure, skips } of units(metafile, treeFile, entries, rest)) {
    console.log(`${root}\t${[...new Set([...skips, ...fetched.filter(p => !closure.has(p))])].join(" ")}`);
  }
};

const narrow = (skipsFile, readsFile) => {
  const reads = readFileSync(readsFile, "utf8").split("\n").filter(l => l !== "");
  const skips = readFileSync(skipsFile, "utf8").split(/\s+/).filter(s => s !== "");
  console.log(skips.filter(s => !reads.some(r => under(s, r))).join(" "));
};

switch (mode) {
  case "crates": crates(args[0], args[1], args[2], args.slice(3)); break;
  case "files": files(args[0], args[1], args[2], args.slice(3)); break;
  case "scenes": scenes(args[0], args[1], args[2], args.slice(3)); break;
  case "narrow": narrow(args[0], args[1]); break;
  default: fail("usage: gate-units.mjs crates <metadata> <root> <tree> [--base <path>...] [<package>=<path>]... | files <metafile> <tree> <entries> --scope <dir>... [--shared [<entry>=]<path>...] [--base <path>...] | scenes <metafile> <tree> <entries> --app <metafile> --static <metafile> [--later <path>...] (and files' flags) | narrow <skips> <reads>");
}
