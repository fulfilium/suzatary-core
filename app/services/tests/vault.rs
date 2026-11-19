//! The vault's folder as Drive's allowance reads it: no allowance opens it.

use suzatary_base::drive_path::DrivePath;
use suzatary_held::vault::{folder, holds};

#[test]
fn the_folder_is_a_drive_path_no_allowance_opens() {
    let inside = folder().join("w2.pdf").expect("a name");
    assert!(!suzatary_services::drive::allowed(
        &["/my-files".parse().expect("a path")],
        &inside
    ));
    assert!(!suzatary_services::drive::allowed(&[folder()], &inside));
}

#[test]
fn the_folder_is_held_whatever_its_case() {
    for path in [
        "/my-files/Suzatary/Private",
        "/my-files/Suzatary/PRIVATE/w2.pdf",
        "/MY-FILES/suzatary/private/a/b.pdf",
    ] {
        let path: DrivePath = path.parse().expect("a path");
        assert!(holds(&path), "{path}");
        assert!(
            !suzatary_services::drive::allowed(&["/my-files".parse().expect("a path")], &path),
            "{path}"
        );
    }
    for path in ["/my-files/Suzatary", "/my-files/Suzatary/Privately/a.pdf"] {
        assert!(!holds(&path.parse().expect("a path")), "{path}");
    }
}
