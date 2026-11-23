//! Typst on the box: authored markup compiled in the closed world (`typeset`), a template
//! rendered to checked HTML (`render`), and the render caches a reader keeps (`rendered`) or
//! keeps none of (`unrendered`).

#![forbid(unsafe_code)]

pub mod render;
pub mod rendered;
pub mod typeset;
pub mod unrendered;
