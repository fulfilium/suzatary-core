//! The route table lists exactly what `surface.golden` lists, so the surface the CLI's tests
//! probe and the table that mounts it are one list.

#[test]
fn the_table_is_the_golden_surface() {
    let golden: Vec<&str> = include_str!("surface.golden")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect();
    assert_eq!(suzatary_server::serve::table::surface(), golden);
}
