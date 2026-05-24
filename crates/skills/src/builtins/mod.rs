pub mod automation;
pub mod browser;
pub mod content_search;
pub mod cron;
pub mod file_edit;
pub mod file_read;
pub mod file_search;
pub mod file_write;
pub mod install_skill;
pub mod schedule_reminder;
pub mod shell;
pub mod update_settings;
pub mod web_browser;

use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use openloom_models::AppConfig;

use crate::cron_store::CronStore;
use crate::SkillRegistry;

pub fn register_all(
    registry: &mut SkillRegistry,
    config: Arc<RwLock<AppConfig>>,
    data_dir: &Path,
    cron_store: Arc<CronStore>,
) {
    registry.register(Box::new(file_read::FileRead));
    registry.register(Box::new(file_write::FileWrite));
    registry.register(Box::new(file_edit::FileEdit));
    registry.register(Box::new(shell::Shell));
    registry.register(Box::new(file_search::FileSearch));
    registry.register(Box::new(content_search::ContentSearch));
    registry.register(Box::new(web_browser::WebBrowser));
    registry.register(Box::new(schedule_reminder::ScheduleReminder));
    registry.register(Box::new(update_settings::UpdateSettingsSkill::new(
        config,
        data_dir.to_path_buf(),
    )));
    registry.register(Box::new(install_skill::InstallSkillSkill::new(
        data_dir.to_path_buf(),
    )));
    registry.register(Box::new(cron::CronSkill::new(cron_store.clone())));
    registry.register(Box::new(automation::AutomationSkill::new(
        cron_store,
        data_dir,
    )));
    registry.register(Box::new(browser::BrowserSkill));
}
