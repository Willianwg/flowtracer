pub mod classifier;
pub mod cli;
pub mod grouper;
pub mod input;
pub mod model;
pub mod parser;
pub mod renderer;
pub mod trace_builder;

pub use classifier::classify_all;
pub use grouper::{group_events, GroupConfig, RequestGroup};
pub use model::span::{Span, SpanKind};
pub use model::{ClassifiedEvent, ErrorDetail, ErrorType, EventKind, LogEvent, LogLevel, Trace};
pub use parser::{plain::PlainTextParser, LogParser};
pub use renderer::colors::ColorConfig;
pub use renderer::tree::TreeRenderer;
pub use renderer::Renderer;
pub use trace_builder::{build_trace, build_traces};
