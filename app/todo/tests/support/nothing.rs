//! The inert renderer: test support, never production.

use suzatary_base::capability::{Nothing, RenderError};
use suzatary_base::escape::Escaped;

use crate::render::{Html, Render};
use crate::render_cache::RenderCache;

impl Render for Nothing {
    fn html(&self, markup: &Escaped) -> Result<Html, RenderError> {
        Ok(Html::from_render(markup.as_str()))
    }
    fn cached(&self, markup: &Escaped) -> Option<Html> {
        Some(Html::from_render(markup.as_str()))
    }
    fn warm(&self, markup: &[Escaped]) -> usize {
        markup.len()
    }
    fn page(&self, page: &crate::template::Called) -> Result<Html, RenderError> {
        Ok(Html::from_render(page.job().state.clone()))
    }
}

/// Nothing is rendered, so nothing is kept to forget.
impl RenderCache for Nothing {
    fn forget(&self, _: &suzatary_base::answer::Renderable) -> Result<(), String> {
        Ok(())
    }
}
