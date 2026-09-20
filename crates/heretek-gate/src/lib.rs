pub mod baseline;
pub mod files;
pub mod pipeline;
pub mod process;
pub mod shadow;
pub mod stage;
pub mod stages;

pub use baseline::Baseline;
pub use pipeline::Pipeline;
pub use shadow::{ShadowWorkspace, apply_from, discard};
pub use stage::{GateContext, GateError, Stage, StageOutcome, Target};
pub use stages::build_pipeline;
