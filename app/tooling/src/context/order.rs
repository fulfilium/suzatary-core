//! The order of a crate's top modules, and the crates a cut of them would make.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Context as _, bail};
use serde::Deserialize;

use super::interface;
use super::measure::{MANUALS, Report, with_cited};
use super::plan::{Plan, Shape, Unit};
use super::tokens::Kind;
use super::tree::Tree;

/// One top module: `src/<name>.rs` and the files under `src/<name>/`.
#[derive(Debug, Default)]
struct Module {
    /// Its production source, in tokens.
    source: u64,
    /// Its public interface, in tokens.
    face: u64,
    /// The crate's other top modules it names.
    names: BTreeSet<String>,
    /// The other units it names.
    units: BTreeSet<String>,
    /// Each dependent unit that names it, with how many of its production files do.
    users: BTreeMap<String, usize>,
}

/// A cut, bottom first: each layer becomes a crate, and the modules no layer lists stay in the
/// unit, above them all.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cut {
    layer: Vec<Layer>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    name: String,
    modules: Vec<String>,
}

impl Cut {
    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        toml::from_str(text).context("reading the cut")
    }
}

/// The report of `unit`'s modules over `tree`, and of `cut` when one is given; an error when the
/// cut names a module the unit lacks or places one twice, or when a module of one layer names a
/// module of a layer above it.
pub(crate) fn order(
    plan: &Plan,
    tree: &Tree,
    report: &Report,
    unit: &str,
    cut: Option<&Cut>,
) -> anyhow::Result<String> {
    let Some(the_unit) = plan.units.iter().find(|u| u.name == unit) else {
        bail!("no unit {unit}");
    };
    let modules = modules(plan, tree, the_unit);
    let mut out = String::new();
    by_module(&mut out, &modules);
    components_of(&mut out, &modules);
    if let Some(cut) = cut {
        layers(&mut out, plan, tree, report, the_unit, &modules, cut)?;
    }
    Ok(out)
}

/// The library name of each unit and external crate, which a path begins with.
fn libraries(plan: &Plan) -> BTreeMap<String, String> {
    plan.units
        .iter()
        .filter(|unit| matches!(unit.shape, Shape::Crate))
        .map(|unit| {
            (
                format!("suzatary_{}", unit.name.replace('-', "_")),
                unit.name.clone(),
            )
        })
        .chain(
            plan.externals()
                .map(|(_, external)| (external.name.replace('-', "_"), external.name.clone())),
        )
        .collect()
}

/// The top module a file of `src/` belongs to; none for the crate's root.
fn top_module(inside_src: &str) -> Option<&str> {
    match inside_src.split_once('/') {
        Some((dir, _)) => Some(dir),
        None => inside_src.strip_suffix(".rs").filter(|name| *name != "lib"),
    }
}

fn modules(plan: &Plan, tree: &Tree, unit: &Unit) -> BTreeMap<String, Module> {
    let tokens = |path: &str, text: &str| plan.tokens.ratio.tokens(Kind::of(path), text);
    let libraries = libraries(plan);
    let src = format!("{}/src/", unit.path);
    let mut modules: BTreeMap<String, Module> = BTreeMap::new();
    for (path, text) in tree.under(&src[..src.len() - 1]) {
        let Some(name) = top_module(&path[src.len()..]) else {
            continue;
        };
        if Kind::of(path) != Kind::Rust {
            continue;
        }
        let code = interface::production(text);
        let module = modules.entry(name.to_owned()).or_default();
        module.source += tokens(path, code);
        module.face += tokens(path, &interface::rust(text));
        module.names.extend(paths_after(code, "crate::"));
        for (library, named) in &libraries {
            if *named != unit.name && !paths_after(code, &format!("{library}::")).is_empty() {
                module.units.insert(named.clone());
            }
        }
    }
    let names: BTreeSet<String> = modules.keys().cloned().collect();
    for (key, module) in &mut modules {
        module
            .names
            .retain(|name| names.contains(name) && name != key);
    }
    let own = format!("suzatary_{}::", unit.name.replace('-', "_"));
    for user in plan.units.iter().filter(|u| u.depends.contains(&unit.name)) {
        let user_src = format!("{}/src", user.path);
        for (_, text) in tree.under(&user_src) {
            for name in paths_after(interface::production(text), &own) {
                if let Some(module) = modules.get_mut(&name) {
                    *module.users.entry(user.name.clone()).or_default() += 1;
                }
            }
        }
    }
    modules
}

/// The first segment of every path in `code` that starts with `prefix`, through braced groups
/// (`prefix{a::X, b::{c, d}}` gives `a` and `b`), comments left out.
fn paths_after(code: &str, prefix: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in code.lines() {
        let line = line.find("//").map_or(line, |at| &line[..at]);
        let mut rest = line;
        while let Some(at) = rest.find(prefix) {
            let before = rest[..at].chars().next_back();
            rest = &rest[at + prefix.len()..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            first_segments(rest, &mut found);
        }
    }
    found.retain(|name| !matches!(name.as_str(), "self" | "super" | "crate" | ""));
    found
}

/// The first segment of a path, or of each path in the braced group `text` opens with.
fn first_segments(text: &str, found: &mut BTreeSet<String>) {
    let word = |text: &str| -> String {
        text.trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    };
    let Some(group) = text.trim_start().strip_prefix('{') else {
        found.insert(word(text));
        return;
    };
    let mut depth = 0usize;
    let mut start = 0;
    for (at, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => {
                found.insert(word(&group[start..at]));
                return;
            }
            '}' => depth -= 1,
            ',' if depth == 0 => {
                found.insert(word(&group[start..at]));
                start = at + 1;
            }
            _ => {}
        }
    }
    found.insert(word(&group[start..]));
}

fn by_module(out: &mut String, modules: &BTreeMap<String, Module>) {
    let _ = writeln!(
        out,
        "{:<16} {:>7} {:>6}  {:<28} {:<40} names",
        "module", "source", "face", "used by", "other units"
    );
    for (name, module) in modules {
        let users: Vec<String> = module
            .users
            .iter()
            .map(|(user, files)| format!("{user}:{files}"))
            .collect();
        let units: Vec<&str> = module.units.iter().map(String::as_str).collect();
        let names: Vec<&str> = module.names.iter().map(String::as_str).collect();
        let _ = writeln!(
            out,
            "{name:<16} {:>7} {:>6}  {:<28} {:<40} {}",
            module.source,
            module.face,
            users.join(" "),
            units.join(" "),
            names.join(" ")
        );
    }
    let mut largest: Vec<(&String, &Module)> = modules.iter().collect();
    largest.sort_by_key(|(name, module)| (std::cmp::Reverse(module.face), *name));
    let total: u64 = modules.values().map(|module| module.face).sum();
    let _ = writeln!(out, "faces: {total} tokens; the ten largest:");
    for (name, module) in largest.iter().take(10) {
        let _ = writeln!(out, "  {:>6}  {name}", module.face);
    }
}

fn components_of(out: &mut String, modules: &BTreeMap<String, Module>) {
    let index: BTreeMap<&str, usize> = modules
        .keys()
        .enumerate()
        .map(|(at, name)| (name.as_str(), at))
        .collect();
    let edges: Vec<Vec<usize>> = modules
        .values()
        .map(|module| {
            module
                .names
                .iter()
                .map(|name| index[name.as_str()])
                .collect()
        })
        .collect();
    let names: Vec<&String> = modules.keys().collect();
    let cycles: Vec<Vec<usize>> = components(&edges)
        .into_iter()
        .filter(|component| component.len() > 1)
        .collect();
    let _ = writeln!(
        out,
        "strongly connected components of more than one module: {}",
        cycles.len()
    );
    for component in cycles {
        let members: Vec<&str> = component.iter().map(|at| names[*at].as_str()).collect();
        let _ = writeln!(out, "  {}", members.join(" "));
    }
}

/// The strongly connected components of the graph whose node `n` has the edges `edges[n]`, each
/// sorted, in an order where every edge leads to its own component or to an earlier one.
fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Walk<'a> {
        edges: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        next: usize,
        found: Vec<Vec<usize>>,
    }
    fn visit(walk: &mut Walk<'_>, node: usize) {
        walk.index[node] = Some(walk.next);
        walk.low[node] = walk.next;
        walk.next += 1;
        walk.stack.push(node);
        walk.on_stack[node] = true;
        for &next in &walk.edges[node] {
            match walk.index[next] {
                None => {
                    visit(walk, next);
                    walk.low[node] = walk.low[node].min(walk.low[next]);
                }
                Some(seen) if walk.on_stack[next] => walk.low[node] = walk.low[node].min(seen),
                Some(_) => {}
            }
        }
        if Some(walk.low[node]) == walk.index[node] {
            let mut component = Vec::new();
            while let Some(member) = walk.stack.pop() {
                walk.on_stack[member] = false;
                component.push(member);
                if member == node {
                    break;
                }
            }
            component.sort_unstable();
            walk.found.push(component);
        }
    }
    let mut walk = Walk {
        edges,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        stack: Vec::new(),
        on_stack: vec![false; edges.len()],
        next: 0,
        found: Vec::new(),
    };
    for node in 0..edges.len() {
        if walk.index[node].is_none() {
            visit(&mut walk, node);
        }
    }
    walk.found
}

/// Where a cut places each module: the index of its layer, the unit itself last.
struct Placed<'a> {
    layer_of: BTreeMap<&'a str, usize>,
    /// Each layer's name, bottom first, then the unit's.
    names: Vec<&'a str>,
}

impl<'a> Placed<'a> {
    fn of(&self, module: &str) -> usize {
        self.layer_of
            .get(module)
            .copied()
            .unwrap_or(self.names.len() - 1)
    }

    fn new(
        unit: &'a Unit,
        modules: &BTreeMap<String, Module>,
        cut: &'a Cut,
    ) -> anyhow::Result<Self> {
        let mut layer_of = BTreeMap::new();
        for (at, layer) in cut.layer.iter().enumerate() {
            for module in &layer.modules {
                if !modules.contains_key(module) {
                    bail!(
                        "the cut's layer {} names {module}, which {} lacks",
                        layer.name,
                        unit.name
                    );
                }
                if layer_of.insert(module.as_str(), at).is_some() {
                    bail!("the cut places {module} twice");
                }
            }
        }
        let names = cut
            .layer
            .iter()
            .map(|layer| layer.name.as_str())
            .chain([unit.name.as_str()])
            .collect();
        Ok(Self { layer_of, names })
    }
}

fn layers(
    out: &mut String,
    plan: &Plan,
    tree: &Tree,
    report: &Report,
    unit: &Unit,
    modules: &BTreeMap<String, Module>,
    cut: &Cut,
) -> anyhow::Result<()> {
    let placed = Placed::new(unit, modules, cut)?;
    let use_of = |name: &str| {
        report.unit(name).map_or_else(
            || plan.external(name).map_or(0, |(_, external)| external.use_),
            |unit| unit.figures.use_,
        )
    };
    let faces = crates(out, plan, tree, unit, modules, &placed, &use_of);
    for user in plan.units.iter().filter(|u| u.depends.contains(&unit.name)) {
        let read: BTreeSet<usize> = modules
            .iter()
            .filter(|(_, module)| module.users.contains_key(&user.name))
            .map(|(name, _)| placed.of(name))
            .collect();
        let now: u64 = read.iter().map(|at| faces[*at]).sum();
        let crates: Vec<&str> = read.iter().map(|at| placed.names[*at]).collect();
        let was = use_of(&unit.name);
        let _ = writeln!(
            out,
            "{} reads {} ({now} tokens) for {} ({was}): {:+}",
            user.name,
            crates.join(" "),
            unit.name,
            i128::from(now) - i128::from(was)
        );
    }
    let mut up = Vec::new();
    for (name, module) in modules {
        for named in &module.names {
            if placed.of(named) > placed.of(name) {
                up.push(format!(
                    "{name} ({}) names {named} ({})",
                    placed.names[placed.of(name)],
                    placed.names[placed.of(named)]
                ));
            }
        }
    }
    if up.is_empty() {
        let _ = writeln!(out, "no module names a module of a layer above its own");
        Ok(())
    } else {
        bail!(
            "{out}{} edge(s) up the cut:\n  {}",
            up.len(),
            up.join("\n  ")
        )
    }
}

/// The table of the crates `placed` makes, each measured as `measure` measures a unit; answers
/// each one's USE.
fn crates(
    out: &mut String,
    plan: &Plan,
    tree: &Tree,
    unit: &Unit,
    modules: &BTreeMap<String, Module>,
    placed: &Placed<'_>,
    use_of: &dyn Fn(&str) -> u64,
) -> Vec<u64> {
    let manuals: Vec<(&str, &str)> = MANUALS
        .iter()
        .filter_map(|path| tree.get(path).map(|text| (*path, text)))
        .collect();
    let src = format!("{}/src/", unit.path);
    let mut faces = vec![0; placed.names.len()];
    let _ = writeln!(
        out,
        "{:<14} {:>4} {:>8} {:>8} {:>9}  depends on",
        "crate", "mods", "source", "use", "develop"
    );
    for (at, name) in placed.names.iter().enumerate() {
        let members: Vec<&Module> = modules
            .iter()
            .filter(|(module, _)| placed.of(module) == at)
            .map(|(_, module)| module)
            .collect();
        let read: Vec<(&str, &str)> = tree
            .under(&src[..src.len() - 1])
            .filter(|(path, _)| {
                top_module(&path[src.len()..]).is_some_and(|module| placed.of(module) == at)
            })
            .map(|(path, text)| (path, interface::production(text)))
            .chain(manuals.iter().copied())
            .collect();
        let mut depends: BTreeSet<&str> = BTreeSet::new();
        for module in &members {
            depends.extend(module.units.iter().map(String::as_str));
            depends.extend(
                module
                    .names
                    .iter()
                    .map(|named| placed.names[placed.of(named)]),
            );
        }
        depends.remove(name);
        let (below, units): (Vec<&str>, Vec<&str>) =
            depends.iter().partition(|dep| placed.names.contains(dep));
        let charged: BTreeSet<&str> = units.iter().flat_map(|dep| plan.charged(dep)).collect();
        let develop = with_cited(&read, plan, tree).total()
            + charged.iter().map(|dep| use_of(dep)).sum::<u64>()
            + below
                .iter()
                .filter_map(|dep| placed.names.iter().position(|layer| layer == dep))
                .map(|layer| faces[layer])
                .sum::<u64>();
        faces[at] = members.iter().map(|module| module.face).sum();
        let source: u64 = members.iter().map(|module| module.source).sum();
        let listed: Vec<&str> = depends.into_iter().collect();
        let _ = writeln!(
            out,
            "{name:<14} {:>4} {source:>8} {:>8} {develop:>9}  {}",
            members.len(),
            faces[at],
            listed.join(" ")
        );
    }
    faces
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::{components, paths_after};

    #[test]
    fn a_path_is_read_through_its_groups() {
        let code = "use crate::{a::X, b::{c, d}, self};\nlet _ = crate::e::f(); // crate::g\npub(crate) fn h() { $crate::i!(); xcrate::j }";
        let found: Vec<String> = paths_after(code, "crate::").into_iter().collect();
        assert_eq!(found, ["a", "b", "e", "i"]);
    }

    fn reaches(edges: &[Vec<usize>], from: usize, to: usize) -> bool {
        let mut seen = BTreeSet::from([from]);
        let mut todo = vec![from];
        while let Some(node) = todo.pop() {
            for &next in &edges[node] {
                if seen.insert(next) {
                    todo.push(next);
                }
            }
        }
        seen.contains(&to)
    }

    proptest! {
        /// Two nodes share a component exactly when each reaches the other, and every edge leads
        /// to its own component or an earlier one: the order `layers` checks a cut against.
        #[test]
        fn components_are_the_mutually_reachable_classes_in_order(
            edges in (1usize..12).prop_flat_map(|n| prop::collection::vec(prop::collection::vec(0..n, 0..4), n))
        ) {
            let found = components(&edges);
            let mut place = vec![usize::MAX; edges.len()];
            for (at, component) in found.iter().enumerate() {
                for &node in component {
                    prop_assert_eq!(place[node], usize::MAX);
                    place[node] = at;
                }
            }
            prop_assert!(place.iter().all(|at| *at != usize::MAX));
            for a in 0..edges.len() {
                for b in 0..edges.len() {
                    let mutual = reaches(&edges, a, b) && reaches(&edges, b, a);
                    prop_assert_eq!(place[a] == place[b], mutual);
                }
                for &b in &edges[a] {
                    prop_assert!(place[b] <= place[a]);
                }
            }
        }
    }
}
