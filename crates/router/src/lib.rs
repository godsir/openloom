pub mod keywords;

use keywords::KeywordRule;
use openloom_models::{ClassifyOutput, Intent, TargetModel};

pub struct RouterConfig {
    pub model_path: std::path::PathBuf,
    pub keyword_rules: Vec<KeywordRule>,
    pub keyword_threshold: f32,
    pub fallback_threshold: f32,
}

pub struct SmartRouter {
    config: RouterConfig,
    skill_triggers: Vec<(String, Vec<String>)>,
    cloud_available: bool,
}

impl SmartRouter {
    /// Create a keyword-only router (Phase 1 initial version; LLM classification added later)
    pub fn new_keywords_only(keyword_rules: Vec<KeywordRule>) -> Self {
        Self {
            config: RouterConfig {
                model_path: std::path::PathBuf::new(),
                keyword_rules,
                keyword_threshold: 0.85,
                fallback_threshold: 0.7,
            },
            skill_triggers: Vec::new(),
            cloud_available: false,
        }
    }

    /// Synchronous classification using keyword matching
    pub fn classify_sync(&self, text: &str) -> ClassifyOutput {
        if text.is_empty() {
            return ClassifyOutput {
                intent: Intent::Chat,
                complexity: 0.0,
                skill_match: None,
                confidence: 1.0,
                cache_hit: false,
                target_model: TargetModel::Local,
                route_reason: "empty_input".to_string(),
            };
        }

        // Step 1: Keyword matching
        let mut best_confidence = 0.0f32;
        let mut best_intent = Intent::Chat;
        for rule in &self.config.keyword_rules {
            if rule.pattern.is_match(text) && rule.confidence > best_confidence {
                best_confidence = rule.confidence;
                best_intent = rule.intent.clone();
            }
        }

        // Step 2: Skill trigger matching
        let mut skill_match = None;
        for (name, triggers) in &self.skill_triggers {
            for trigger in triggers {
                if text.contains(trigger.as_str()) {
                    skill_match = Some(name.clone());
                    break;
                }
            }
            if skill_match.is_some() {
                break;
            }
        }

        let (target_model, complexity, reason) = if best_confidence >= self.config.keyword_threshold
        {
            if skill_match.is_some() {
                (TargetModel::None, 0.3, "skill_trigger")
            } else {
                (TargetModel::Local, 0.3, "keyword_match")
            }
        } else if best_confidence >= self.config.fallback_threshold {
            (TargetModel::Local, 0.6, "keyword_fallback")
        } else if self.cloud_available {
            (TargetModel::Cloud, 0.8, "cloud_fallback")
        } else {
            (TargetModel::Local, 0.8, "default_local")
        };

        ClassifyOutput {
            intent: best_intent,
            complexity,
            skill_match,
            confidence: best_confidence.max(0.3),
            cache_hit: false,
            target_model,
            route_reason: reason.to_string(),
        }
    }

    /// Register skill trigger words for matching
    pub fn register_skill_triggers(&mut self, name: &str, triggers: Vec<String>) {
        self.skill_triggers.push((name.to_string(), triggers));
    }

    /// Set whether a cloud model is available for routing
    pub fn set_cloud_available(&mut self, available: bool) {
        self.cloud_available = available;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openloom_models::{Intent, TargetModel};

    #[test]
    fn test_classify_file_operation_keyword() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("打开这个文件看看");
        assert_eq!(output.intent, Intent::FileOperation);
        assert!(output.confidence >= 0.85);
        assert_eq!(output.target_model, TargetModel::Local);
        assert_eq!(output.route_reason, "keyword_match");
    }

    #[test]
    fn test_classify_code_assist_keyword() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("帮我写一个Python脚本处理CSV");
        assert_eq!(output.intent, Intent::CodeAssist);
        assert!(output.confidence >= 0.80);
    }

    #[test]
    fn test_classify_chat_fallback() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("你好啊，很高兴见到你");
        assert_eq!(output.intent, Intent::Chat);
        assert_eq!(output.target_model, TargetModel::Local);
        assert!(
            output.route_reason == "keyword_fallback" || output.route_reason == "default_local"
        );
    }

    #[test]
    fn test_route_reason_on_empty_input() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("");
        assert_eq!(output.route_reason, "empty_input");
    }

    #[test]
    fn test_empty_input() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("");
        assert_eq!(output.intent, Intent::Chat);
    }

    #[test]
    fn test_register_skill_triggers() {
        let mut router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        router.register_skill_triggers("file-manager", vec!["文件".into(), "文档".into()]);
        let output = router.classify_sync("帮我管理文件");
        assert_eq!(output.skill_match, Some("file-manager".into()));
    }

    #[test]
    fn test_classify_web_search() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("搜索一下最新的Rust新闻");
        assert_eq!(output.intent, Intent::WebSearch);
        assert!(output.confidence >= 0.75);
    }

    #[test]
    fn test_classify_schedule() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("提醒我明天下午三点开会");
        assert_eq!(output.intent, Intent::Schedule);
        assert!(output.confidence >= 0.85);
    }

    #[test]
    fn test_classify_question() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("为什么会下雨");
        assert_eq!(output.intent, Intent::Question);
    }
}
