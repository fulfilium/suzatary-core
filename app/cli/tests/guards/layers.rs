//! The core's layers, held until each is a crate: a module of `app/core/src` names its own cut's
//! modules and lower cuts', never a higher one's (`guards/layers.toml`). A path is judged
//! by the module file it reaches, so `use crate::wire;` then `wire::todo::Entry` reaches
//! `wire/todo.rs`. The edges that still point up are listed by name, and the list only shrinks:
//! a listed edge that no longer occurs fails too, so its line goes with it.
//!
//! The sources `rendering.rs` hashes into the name of every kept render are pinned by digest
//! here as well: a change to one renames every kept render, so it says so where it is reviewed.

use super::repo;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The `[layers]` table: the cuts bottom first, the modules of each, the edges still allowed,
/// and the digest of each source `rendering.rs` hashes.
#[derive(serde::Deserialize)]
struct Layers {
    order: Vec<String>,
    modules: BTreeMap<String, Vec<String>>,
    allowed: Vec<Edge>,
    rendering: Vec<Hashed>,
}

/// An edge still allowed to point up.
#[derive(serde::Deserialize)]
struct Edge {
    from: String,
    to: String,
}

/// A source `rendering.rs` hashes, and its digest.
#[derive(serde::Deserialize)]
struct Hashed {
    file: String,
    sha256: String,
}

fn layers() -> Layers {
    #[derive(serde::Deserialize)]
    struct File {
        layers: Layers,
    }
    let text = std::fs::read_to_string(repo().join("app/cli/tests/guards/layers.toml"))
        .expect("layers.toml");
    toml::from_str::<File>(&text)
        .expect("guards/layers.toml")
        .layers
}

fn core() -> PathBuf {
    repo().join("app/core/src")
}

/// A source file's module path under `app/core/src`: `secretary/verb_name`, `secretary` for
/// `secretary/mod.rs`, `wire` for `wire.rs`.
fn module_of(file: &Path) -> String {
    let rel = file.strip_prefix(core()).expect("under the core");
    let text = rel.with_extension("").to_string_lossy().replace('\\', "/");
    text.strip_suffix("/mod")
        .map_or(text.clone(), str::to_owned)
}

/// The module whose file a path of segments reaches: its longest prefix that is a module file.
fn reached(segments: &[String]) -> Option<String> {
    (1..=segments.len()).rev().find_map(|n| {
        let path = segments[..n].join("/");
        let dir = core().join(&path);
        (dir.with_extension("rs").is_file() || dir.join("mod.rs").is_file()).then_some(path)
    })
}

/// The cut of a module: that of the longest listed module path at or above it.
fn cut_of<'a>(module: &str, owner: &'a BTreeMap<String, String>) -> Option<&'a String> {
    let parts: Vec<&str> = module.split('/').collect();
    (1..=parts.len())
        .rev()
        .find_map(|n| owner.get(&parts[..n].join("/")))
}

/// The text before the file's `#[cfg(test)]` module, with `//` comments gone: a `#[cfg(test)]` on
/// one item does not end the production code.
fn production(text: &str) -> String {
    let before = text
        .match_indices("\n#[cfg(test)]\n")
        .find(|(at, _)| {
            let item = text[at + "\n#[cfg(test)]\n".len()..].trim_start();
            let item = item
                .strip_prefix("pub(crate) ")
                .or_else(|| item.strip_prefix("pub "))
                .unwrap_or(item);
            item.starts_with("mod ")
        })
        .map_or(text, |(at, _)| &text[..at]);
    before
        .lines()
        .map(|line| {
            let mut quotes = 0;
            let bytes = line.as_bytes();
            for i in 0..bytes.len() {
                match bytes[i] {
                    b'"' if i == 0 || bytes[i - 1] != b'\\' => quotes += 1,
                    b'/' if quotes % 2 == 0 && bytes.get(i + 1) == Some(&b'/') => {
                        return &line[..i];
                    }
                    _ => {}
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every leaf of a `use` tree, as segments and the name it binds.
fn leaves(tree: &str) -> Vec<(Vec<String>, String)> {
    fn go(tree: &str, prefix: &[String], into: &mut Vec<(Vec<String>, String)>) {
        let tree = tree.trim();
        if let Some(open) = tree.find('{') {
            let head: Vec<String> = tree[..open]
                .split("::")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let base: Vec<String> = prefix.iter().cloned().chain(head).collect();
            let inner = &tree[open + 1..tree.rfind('}').expect("a closed tree")];
            let (mut depth, mut start) = (0, 0);
            for (i, c) in inner.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        go(&inner[start..i], &base, into);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            go(&inner[start..], &base, into);
            return;
        }
        let mut parts = tree.split(" as ");
        let path = parts.next().unwrap_or(tree).trim();
        let alias = parts.next().map(str::trim);
        if path.is_empty() || path == "*" {
            if path == "*" {
                into.push((prefix.to_vec(), String::new()));
            }
            return;
        }
        let mut segments = prefix.to_vec();
        for segment in path.split("::").map(str::trim) {
            if segment == "self" {
                continue;
            }
            segments.push(segment.to_owned());
        }
        let name = alias
            .map(str::to_owned)
            .or_else(|| segments.last().cloned())
            .unwrap_or_default();
        into.push((segments, name));
    }
    let mut out = Vec::new();
    go(tree, &[], &mut out);
    out
}

/// The inline modules (`mod name {`) open at each byte of `text`, innermost last.
fn inline_modules(text: &str) -> Vec<(usize, Vec<String>)> {
    let mut marks = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let line_start = text[..i].rfind('\n').map_or(0, |n| n + 1);
                let head = text[line_start..i].trim();
                let head = head
                    .strip_prefix("pub(crate) ")
                    .or_else(|| head.strip_prefix("pub "))
                    .unwrap_or(head);
                if let Some(name) = head.strip_prefix("mod ") {
                    stack.push((name.trim().to_owned(), depth));
                    marks.push((i, stack.iter().map(|(n, _)| n.clone()).collect()));
                }
                depth += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if stack.last().is_some_and(|(_, d)| *d == depth) {
                    stack.pop();
                    marks.push((i, stack.iter().map(|(n, _)| n.clone()).collect()));
                }
            }
            _ => {}
        }
        i += 1;
    }
    marks
}

/// The paths `text` names from the crate's root: its `use` trees, its inline `crate::` and
/// `super::` paths (`super` read from the module at that point, inline modules included), and
/// what follows each module a `use` binds, so a path is judged by the item it reaches and never
/// by a module bound on the way.
fn paths(module: &str, text: &str) -> Vec<Vec<String>> {
    let own: Vec<String> = module.split('/').map(str::to_owned).collect();
    let marks = inline_modules(text);
    let here = |at: usize| -> Vec<String> {
        let inner = marks
            .iter()
            .take_while(|(m, _)| *m <= at)
            .last()
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        own.iter().cloned().chain(inner).collect()
    };
    let absolute = |at: usize, segments: Vec<String>| -> Option<Vec<String>> {
        match segments.first().map(String::as_str) {
            Some("crate") => Some(segments[1..].to_vec()),
            Some("super") => {
                let mut base = here(at);
                let mut rest = segments.as_slice();
                while rest.first().is_some_and(|s| s == "super") {
                    base.pop();
                    rest = &rest[1..];
                }
                Some(base.into_iter().chain(rest.iter().cloned()).collect())
            }
            _ => None,
        }
    };
    let mut found = Vec::new();
    let mut bound: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rest = 0;
    while let Some(off) = text[rest..].find("use ") {
        let at = rest + off;
        let boundary = at == 0 || !text.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = &text[at + 4..];
        if boundary && (after.starts_with("crate::") || after.starts_with("super::")) {
            let end = after.find(';').unwrap_or(after.len());
            let tree = after[..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            for (segments, name) in leaves(&tree) {
                let Some(path) = absolute(at, segments) else {
                    continue;
                };
                if reached(&path).is_some_and(|m| m.split('/').count() == path.len()) {
                    bound.insert(name, path);
                } else {
                    found.push(path);
                }
            }
        }
        rest = at + 4;
    }
    let mut roots: Vec<(String, Option<Vec<String>>)> =
        vec![("crate::".to_owned(), None), ("super::".to_owned(), None)];
    roots.extend(
        bound
            .into_iter()
            .map(|(name, path)| (format!("{name}::"), Some(path))),
    );
    for (root, binding) in roots {
        for (at, _) in text.match_indices(&root) {
            if at > 0 {
                let before = text.as_bytes()[at - 1];
                if before.is_ascii_alphanumeric() || before == b'_' || before == b':' {
                    continue;
                }
            }
            let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
            if text[line_start..at].trim_start().starts_with("use ")
                || text[line_start..at].trim_start().starts_with("pub use ")
            {
                continue;
            }
            let path: String = text[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            let segments: Vec<String> = path
                .split("::")
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            if segments.len() > 1 {
                match &binding {
                    Some(base) => found.push(
                        base.iter()
                            .cloned()
                            .chain(segments[1..].iter().cloned())
                            .collect(),
                    ),
                    None => found.extend(absolute(at, segments)),
                }
            }
        }
    }
    found
}

fn sources(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("a readable directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

/// Every edge from a module to a module of a higher cut, as `from -> to`.
fn upward(layers: &Layers) -> BTreeSet<String> {
    let rank: BTreeMap<&String, usize> = layers
        .order
        .iter()
        .enumerate()
        .map(|(i, c)| (c, i))
        .collect();
    let owner: BTreeMap<String, String> = layers
        .modules
        .iter()
        .flat_map(|(cut, modules)| modules.iter().map(move |m| (m.clone(), cut.clone())))
        .collect();
    let mut files = Vec::new();
    sources(&core(), &mut files);
    let mut edges = BTreeSet::new();
    for file in files {
        let module = module_of(&file);
        if module == "lib" {
            continue;
        }
        let from = cut_of(&module, &owner).unwrap_or_else(|| {
            panic!("{module} is in no cut of guards/layers.toml's [layers.modules]")
        });
        let text = std::fs::read_to_string(&file).expect("a readable source");
        for path in paths(&module, &production(&text)) {
            let Some(to_module) = reached(&path) else {
                continue;
            };
            if to_module == module {
                continue;
            }
            let to = cut_of(&to_module, &owner)
                .unwrap_or_else(|| panic!("{to_module} is in no cut of [layers.modules]"));
            if rank[to] > rank[from] {
                edges.insert(format!("{module} -> {to_module}"));
            }
        }
    }
    edges
}

/// No module names a higher cut's, but for the edges `[layers] allowed` lists, each of which
/// still occurs.
#[test]
fn no_module_names_a_higher_cut_s() {
    let layers = layers();
    let found = upward(&layers);
    let allowed: BTreeSet<String> = layers
        .allowed
        .iter()
        .map(|edge| format!("{} -> {}", edge.from, edge.to))
        .collect();
    let new: Vec<&String> = found.difference(&allowed).collect();
    let gone: Vec<&String> = allowed.difference(&found).collect();
    assert!(
        new.is_empty(),
        "these modules name a higher cut's (guards/layers.toml); move the item down or the \
         user up: {new:#?}"
    );
    assert!(
        gone.is_empty(),
        "these allowed edges no longer occur; take them out of [layers] allowed: {gone:#?}"
    );
}

/// Every source `rendering.rs` hashes into a kept render's name is pinned by its digest, so a
/// change to one is seen where it is reviewed: it renames every kept render.
#[test]
fn the_sources_a_render_is_named_by_are_pinned() {
    let layers = layers();
    let rendering = core().join("rendering.rs");
    let text = std::fs::read_to_string(&rendering).expect("rendering.rs");
    let mut hashed = BTreeMap::new();
    for (at, _) in text.match_indices("include_str!(\"") {
        let rest = &text[at + "include_str!(\"".len()..];
        let literal = &rest[..rest.find('"').expect("a closed literal")];
        let bytes = std::fs::read(core().join(literal)).expect("a hashed source");
        hashed.insert(literal.to_owned(), format!("{:x}", Sha256::digest(bytes)));
    }
    assert_eq!(
        hashed,
        layers
            .rendering
            .iter()
            .map(|h| (h.file.clone(), h.sha256.clone()))
            .collect::<BTreeMap<_, _>>(),
        "a source rendering.rs hashes changed, or the list did: every kept render is renamed. \
         Say so in the pull request, then set guards/layers.toml's `rendering` to the left side"
    );
}
