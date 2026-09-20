pub mod pipeline;
pub mod stage;

pub use pipeline::Pipeline;
pub use stage::{GateContext, GateError, Stage, StageOutcome, Target};
