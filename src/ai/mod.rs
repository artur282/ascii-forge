//! AI scene planning: high-level plans, provider clients, compilation and an
//! optional vision review loop.

pub mod compile;
pub mod generate;
pub mod plan;
pub mod provider;
pub mod vision;

pub use compile::{CompileError, compile_plan};
pub use generate::{
    GenerateError, GeneratedPlan, generate_plan, generation_cache_key, scene_plan_schema,
};
pub use plan::{
    Aspect, Beat, BeatTarget, Layout, PlanEffect, PlanValidationError, ScenePlan, TemplateKind,
};
