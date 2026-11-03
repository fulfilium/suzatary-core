//! The renderer a tab's fold holds: none. A row's fields carry their keys and no HTML, which the
//! tab reads back from what this device kept of the box's renders.

use suzatary_core::capability::{Html, Rejected, Render, RenderError, Typeset};
use suzatary_core::escape::Escaped;
use suzatary_core::template::Called;

/// Compiles nothing and holds nothing.
pub struct Unrendered;

const NONE: &str = "the tab's fold renders nothing";

impl Typeset for Unrendered {
    fn closed(&self, _source: &str) -> Result<String, Rejected> {
        Err(Rejected::whole(NONE))
    }
}

impl Render for Unrendered {
    fn html(&self, _markup: &Escaped) -> Result<Html, RenderError> {
        Err(RenderError::Refused(NONE.into()))
    }

    fn cached(&self, _markup: &Escaped) -> Option<Html> {
        None
    }

    fn warm(&self, _markup: &[Escaped]) -> usize {
        0
    }

    fn page(&self, _page: &Called) -> Result<Html, RenderError> {
        Err(RenderError::Refused(NONE.into()))
    }
}
