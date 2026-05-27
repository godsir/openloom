// Loom core-skills — minimal model-only port for TUI compilation.
// Full implementation pending deeper type alignment.

pub mod model;

// Re-export only what the TUI needs
pub use model::SkillDependencies;
pub use model::SkillInterface;
pub use model::SkillMetadata;
pub use model::SkillToolDependency;

// Stub types for the rest of the API surface
pub struct SkillsManager;
pub struct SkillsLoadInput;

pub mod config_rules { /* stub */
}
pub mod injection { /* stub */
}
pub mod manager { /* stub */
}
pub mod loader { /* stub */
}
pub mod remote { /* stub */
}
pub mod render {
    pub struct AvailableSkills;
    pub struct SkillMetadataBudget;
    pub struct SkillRenderReport;
    pub fn build_available_skills() -> AvailableSkills {
        AvailableSkills
    }
    pub fn default_skill_metadata_budget() -> SkillMetadataBudget {
        SkillMetadataBudget
    }
    pub fn render_available_skills_body() -> String {
        String::new()
    }
    pub const SKILLS_HOW_TO_USE_WITH_ABSOLUTE_PATHS: &str = "";
    pub const SKILLS_HOW_TO_USE_WITH_ALIASES: &str = "";
    pub const SKILLS_INTRO_WITH_ABSOLUTE_PATHS: &str = "";
    pub const SKILLS_INTRO_WITH_ALIASES: &str = "";
}
pub mod system { /* stub */
}
pub mod invocation_utils {
    pub fn build_implicit_skill_path_indexes() {}
    pub fn detect_implicit_skill_invocation_for_command() -> Option<super::model::SkillMetadata> {
        None
    }
}
pub mod mention_counts {
    pub fn build_skill_name_counts() {}
}
pub use model::SkillError;
pub use model::SkillLoadOutcome;
pub use model::SkillPolicy;
pub use model::filter_skill_load_outcome_for_product;
