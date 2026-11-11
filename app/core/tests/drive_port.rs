//! The Drive's conformance suite over the in-memory Drive, on generated cases, and the suite
//! catching an implementation that breaks a law.

use proptest::prelude::*;
use suzatary_core::conformance::drive::{Case, Law, check};
use suzatary_core::drive::{DriveEntry, Memory, Port, WritePort};
use suzatary_core::drive_path::DriveFailure;
use suzatary_core::drive_path::DrivePath;

fn cases() -> impl Strategy<Value = Vec<Case>> {
    prop::collection::vec(
        (
            "[a-z0-9]{1,8}",
            "f-[A-Za-z0-9 ._-]{0,12}",
            prop::collection::vec(any::<u8>(), 0..256),
        ),
        1..4,
    )
    .prop_map(|cases| {
        cases
            .into_iter()
            .enumerate()
            .map(|(n, (folder, name, bytes))| Case {
                folder: format!("/my-files/case-{n}-{folder}")
                    .parse()
                    .expect("a Drive path"),
                name,
                bytes,
            })
            .collect()
    })
}

proptest! {
    #[test]
    fn the_in_memory_drive_keeps_every_law(cases in cases()) {
        let drive = Memory::default();
        drive.create_folder(&DrivePath::root(), "my-files").expect("a section");
        prop_assert_eq!(check(&drive, &cases), Ok(()));
    }
}

/// A Drive whose second upload of a name fails: the suite names the law.
struct RefusesSecond(Memory);

impl Port for RefusesSecond {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        self.0.list(folder)
    }
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        self.0.download(file)
    }
}

impl WritePort for RefusesSecond {
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure> {
        self.0.create_folder(parent, name)
    }
    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure> {
        let path = parent.join(name).expect("a name");
        if self.0.download(&path).is_ok() {
            return Err(DriveFailure::Unreachable("already there".to_owned()));
        }
        self.0.upload(parent, name, bytes)
    }
}

#[test]
fn a_drive_that_refuses_a_second_keep_breaks_never_replaces() {
    let drive = RefusesSecond(Memory::default());
    drive
        .create_folder(&DrivePath::root(), "my-files")
        .expect("a section");
    let case = Case {
        folder: "/my-files/a".parse().expect("a path"),
        name: "f".to_owned(),
        bytes: b"x".to_vec(),
    };
    let broken = check(&drive, &[case]).expect_err("a law breaks");
    assert_eq!(broken.law, Law::NeverReplaces);
}

/// A Drive that hands back other bytes: the first law breaks.
struct Forgets(Memory);

impl Port for Forgets {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        self.0.list(folder)
    }
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        self.0.download(file).map(|_| Vec::new())
    }
}

impl WritePort for Forgets {
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure> {
        self.0.create_folder(parent, name)
    }
    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure> {
        self.0.upload(parent, name, bytes)
    }
}

#[test]
fn a_drive_that_forgets_its_bytes_breaks_the_round_trip() {
    let drive = Forgets(Memory::default());
    drive
        .create_folder(&DrivePath::root(), "my-files")
        .expect("a section");
    let case = Case {
        folder: "/my-files/a".parse().expect("a path"),
        name: "f".to_owned(),
        bytes: b"x".to_vec(),
    };
    assert_eq!(
        check(&drive, &[case]).expect_err("a law breaks").law,
        Law::RoundTrip
    );
}
