use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::drive::Port as _;
use crate::secretary::verbs::{drive_path, drive_refusal};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// One folder of the owner's Drive.
pub struct DriveList;

#[derive(Deserialize, JsonSchema)]
pub struct DriveListArgs {
    path: String,
}

impl Verb for DriveList {
    type Surface = SecretarySurface;
    type Args = DriveListArgs;
    type Answer = Value;
    const NAME: &'static str = "drive_list";
    const DESCRIPTION: &'static str = "LIST A FOLDER of the owner's Proton Drive: each entry's path, kind (file \
        or folder), size and date. `/` lists the sections: their own files \
        are under `/my-files`, folders shared with them under \
        `/shared-with-me`. Names only, nothing opened, no tap.";

    fn run(call: &Call<'_>, args: DriveListArgs) -> Result<Value, Refusal> {
        let folder = drive_path(&args.path)?;
        let entries = call
            .caps
            .drive
            .fenced()
            .list(&folder)
            .map_err(|failed| drive_refusal(&failed))?;
        Ok(json!({
            "path": folder.to_string(),
            "entries": entries.iter().map(|entry| json!({
                "path": entry.path.to_string(),
                "kind": entry.kind.as_str(),
                "size": entry.size,
                "modified": entry.modified,
            })).collect::<Vec<_>>(),
        }))
    }
}
