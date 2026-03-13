pub mod colors;
pub mod tree;

use std::io::Write;

use crate::model::Trace;

/// Trait for rendering traces to an output stream.
pub trait Renderer {
    fn render(&self, trace: &Trace, writer: &mut dyn Write) -> anyhow::Result<()>;
}
