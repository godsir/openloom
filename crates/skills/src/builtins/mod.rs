pub mod code_assistant;
pub mod file_manager;
pub mod info_retriever;
pub mod schedule_reminder;
pub mod web_browser;

use crate::SkillRegistry;

pub fn register_all(registry: &mut SkillRegistry) {
    registry.register(Box::new(file_manager::FileManager));
    registry.register(Box::new(info_retriever::InfoRetriever));
    registry.register(Box::new(schedule_reminder::ScheduleReminder));
    registry.register(Box::new(code_assistant::CodeAssistant));
    registry.register(Box::new(web_browser::WebBrowser));
}
