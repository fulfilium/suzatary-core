//! Every template against its type's generated schema: the build's check that a template reads
//! only fields its type has, and that a field its type sends is read.

use std::collections::BTreeMap;

use suzatary_base::tripwire::suspicious;
use suzatary_core::rendering;
use suzatary_core::template::{Checked, Job};
use suzatary_io::typeset::compile;

/// Whether every example of `template` compiles, with `world` in place of its own files.
fn compiles(template: &dyn Checked, world: &BTreeMap<String, String>) -> bool {
    template.examples().into_iter().all(|job| {
        compile(&Job {
            files: world.clone(),
            ..job
        })
        .is_ok_and(|html| suspicious(&html).is_none())
    })
}

/// The world of `template`'s examples.
fn world(template: &dyn Checked) -> BTreeMap<String, String> {
    template
        .examples()
        .into_iter()
        .next()
        .map(|job| job.files)
        .unwrap_or_default()
}

#[test]
fn every_template_compiles_against_every_example_of_its_type() {
    for template in rendering::all() {
        let failed: Vec<String> = template
            .examples()
            .into_iter()
            .filter_map(|job| {
                compile(&job)
                    .err()
                    .map(|refused| format!("{}: {}", job.state, refused.said(job.main_text())))
            })
            .collect();
        assert!(failed.is_empty(), "{}: {failed:#?}", template.main());
        assert!(!template.examples().is_empty(), "{}", template.main());
    }
}

/// Removing any accessor from a type's generated module fails some template that imports it:
/// a template that names a field its type lacks does not compile, and every field a type sends
/// is read by a template.
#[test]
fn removing_any_field_fails_the_build() {
    let templates = rendering::all();
    let mut modules: BTreeMap<String, String> = BTreeMap::new();
    for template in templates {
        modules.extend(
            world(template)
                .into_iter()
                .filter(|(path, _)| path.starts_with("/schema/")),
        );
    }
    assert!(
        modules.values().any(|module| module.contains("#let ")),
        "no accessor to remove"
    );
    for (path, module) in &modules {
        for line in module.lines().filter(|line| line.starts_with("#let ")) {
            let without = module
                .lines()
                .filter(|other| other != &line)
                .collect::<Vec<_>>()
                .join("\n");
            let fails = templates.iter().any(|template| {
                let mut world = world(*template);
                world.contains_key(path)
                    && world.insert(path.clone(), without.clone()).is_some()
                    && !compiles(*template, &world)
            });
            assert!(fails, "{path}: no template fails without `{line}`");
        }
    }
}
