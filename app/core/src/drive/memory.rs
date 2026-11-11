//! A Drive held in memory: the reference implementation the conformance suite is read against.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use super::{DriveEntry, DrivePath, EntryKind, Port, WritePort};
use suzatary_base::drive_path::DriveFailure;

/// A Drive as a map from path to node, `/` always a folder. Its meaning is the map: a folder
/// lists exactly the nodes one segment under it, and a file downloads as the bytes it holds.
#[derive(Debug, Default)]
pub struct Memory(Mutex<BTreeMap<DrivePath, Node>>);

#[derive(Debug, Clone)]
enum Node {
    Folder,
    File(Vec<u8>),
}

impl Memory {
    fn node(nodes: &BTreeMap<DrivePath, Node>, path: &DrivePath) -> Option<Node> {
        if path.is_root() {
            Some(Node::Folder)
        } else {
            nodes.get(path).cloned()
        }
    }

    /// Insert `node` at `parent/name` unless something is there, `parent` being a folder.
    fn put(&self, parent: &DrivePath, name: &str, node: Node) -> Result<(), DriveFailure> {
        let path = parent
            .join(name)
            .map_err(|failed| DriveFailure::Refused(failed.to_string()))?;
        let mut nodes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::node(&nodes, parent) {
            Some(Node::Folder) => {}
            Some(Node::File(_)) => {
                return Err(DriveFailure::Refused(format!("{parent} is a file")));
            }
            None => return Err(DriveFailure::Missing(parent.to_string())),
        }
        nodes.entry(path).or_insert(node);
        Ok(())
    }
}

impl Port for Memory {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        let nodes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::node(&nodes, folder) {
            Some(Node::Folder) => Ok(nodes
                .iter()
                .filter(|(path, _)| path.parent().as_ref() == Some(folder))
                .map(|(path, node)| DriveEntry {
                    path: path.clone(),
                    kind: match node {
                        Node::Folder => EntryKind::Folder,
                        Node::File(_) => EntryKind::File,
                    },
                    size: match node {
                        Node::Folder => None,
                        Node::File(bytes) => u64::try_from(bytes.len()).ok(),
                    },
                    modified: None,
                })
                .collect()),
            Some(Node::File(_)) => Err(DriveFailure::Refused(format!("{folder} is a file"))),
            None => Err(DriveFailure::Missing(folder.to_string())),
        }
    }

    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        let nodes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::node(&nodes, file) {
            Some(Node::File(bytes)) => Ok(bytes),
            Some(Node::Folder) => Err(DriveFailure::Refused(format!("{file} is a folder"))),
            None => Err(DriveFailure::Missing(file.to_string())),
        }
    }
}

impl WritePort for Memory {
    fn create_folder(&self, parent: &DrivePath, name: &str) -> Result<(), DriveFailure> {
        self.put(parent, name, Node::Folder)
    }

    fn upload(&self, parent: &DrivePath, name: &str, bytes: &[u8]) -> Result<(), DriveFailure> {
        self.put(parent, name, Node::File(bytes.to_vec()))
    }
}
