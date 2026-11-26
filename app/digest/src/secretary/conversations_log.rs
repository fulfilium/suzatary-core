//! The conversations' digest with the lines its leaves are: the record, as [`Log`] reads it.

use std::collections::BTreeMap;

use prodrome_data::event::Hash;
use prodrome_digest::{Node, Tree, View};

use crate::log::{Kinded, Log};
use crate::secretary::digest::Digest;
use crate::secretary::digest_label::Label;
use crate::secretary::digest_line::Line;
use crate::secretary::digest_view::{kinds, rendered, whole};
use suzatary_conversations::secretary::store::Entry;

#[derive(Debug, Clone, Copy)]
pub struct Conversations<'d, 'e> {
    digest: &'d Digest,
    entries: &'d BTreeMap<&'e Hash, &'e Entry>,
}

impl<'d, 'e> Conversations<'d, 'e> {
    #[must_use]
    pub fn new(digest: &'d Digest, entries: &'d BTreeMap<&'e Hash, &'e Entry>) -> Self {
        Self { digest, entries }
    }

    fn entry(&self, leaf: &Hash) -> Option<&Entry> {
        self.entries.get(self.digest.line(leaf)).copied()
    }
}

impl Kinded for Label {
    fn kinds(self) -> String {
        kinds(self)
    }
}

impl Log for Conversations<'_, '_> {
    type Label = Label;

    fn tree(&self) -> Option<&Tree<Label>> {
        self.digest.tree()
    }

    fn node(&self, name: &Hash) -> Option<&Node<Label>> {
        self.digest.node(name)
    }

    fn view(&self, budget: u64) -> Option<View> {
        self.digest.view(budget)
    }

    fn summary(&self, node: &Hash) -> Option<&Line> {
        self.digest.summary(node)
    }

    fn label(&self, node: &Node<Label>) -> Label {
        self.digest.label(node)
    }

    fn line(&self, leaf: &Hash) -> Option<String> {
        self.entry(leaf).map(rendered)
    }

    fn whole(&self, leaf: &Hash) -> Option<String> {
        self.entry(leaf).map(whole)
    }
}
