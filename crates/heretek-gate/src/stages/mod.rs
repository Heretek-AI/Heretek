pub mod ast_grep;
pub mod deps;
pub mod format;
pub mod knip;
pub mod semgrep;
pub mod syntax;
pub mod tests;
pub mod typecheck;
pub mod util;

use std::path::Path;

use heretek_core::GateConfig;

use crate::pipeline::Pipeline;
use crate::stage::Stage;

pub fn build_pipeline(repo_root: &Path, config: &GateConfig) -> Pipeline {
    let stages = config.stages.clone();
    let mut pipeline: Vec<Box<dyn Stage>> = Vec::new();

    if stages.syntax {
        pipeline.push(Box::new(syntax::SyntaxStage::new(repo_root, config)));
    }
    if stages.format {
        pipeline.push(Box::new(format::FormatStage::new(repo_root, config)));
    }
    if stages.typecheck {
        pipeline.push(Box::new(typecheck::TypecheckStage::new(repo_root, config)));
    }
    if stages.tests {
        pipeline.push(Box::new(tests::TestsStage::new(repo_root, config)));
    }
    if stages.ast_grep {
        pipeline.push(Box::new(ast_grep::AstGrepStage::new(repo_root, config)));
    }
    if stages.secrets {
        pipeline.push(Box::new(semgrep::SecretsStage::new(repo_root, config)));
    }
    if stages.sast {
        pipeline.push(Box::new(semgrep::SastStage::new(repo_root, config)));
    }
    if stages.dead_code {
        pipeline.push(Box::new(knip::KnipStage::new(repo_root, config)));
    }
    if stages.deps {
        pipeline.push(Box::new(deps::DepsStage::new(repo_root, config)));
    }

    Pipeline::new(pipeline)
}
