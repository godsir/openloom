pub mod content_search;
pub mod file_edit;
pub mod file_read;
pub mod file_search;
pub mod file_write;
pub mod schedule_reminder;
pub mod shell;
pub mod web_browser;

use crate::SkillRegistry;

pub fn register_all(registry: &mut SkillRegistry) {
    registry.register(Box::new(file_read::FileRead));
    registry.register(Box::new(file_write::FileWrite));
    registry.register(Box::new(file_edit::FileEdit));
    registry.register(Box::new(shell::Shell));
    registry.register(Box::new(file_search::FileSearch));
    registry.register(Box::new(content_search::ContentSearch));
    registry.register(Box::new(web_browser::WebBrowser));
    registry.register(Box::new(schedule_reminder::ScheduleReminder));
}
