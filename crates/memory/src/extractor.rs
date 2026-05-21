use crate::event::{Event, EventType};
use regex::Regex;

pub struct ExtractionRule {
    pub pattern: Regex,
    pub event_type: EventType,
    pub action: String,
    pub min_confidence: f64,
}

pub struct RuleBasedExtractor {
    rules: Vec<ExtractionRule>,
}

impl RuleBasedExtractor {
    pub fn new(rules: Vec<ExtractionRule>) -> Self {
        Self { rules }
    }

    pub fn with_default_rules() -> Self {
        let rules = vec![
            // ── 兴趣爱好 ──
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|爱好|感兴趣|热爱|着迷|痴迷).{0,10}(编程|音乐|游戏|运动|读书|旅行|摄影|画画|烹饪|电影|动漫|写作|健身|钓鱼|园艺)").unwrap(),
                event_type: EventType::Preference,
                action: "interest_hobby".into(),
                min_confidence: 0.80,
            },
            // ── 职业身份 ──
            ExtractionRule {
                pattern: Regex::new(r"(我是|我做|我在).{0,6}(工程师|开发|设计师|产品经理|学生|老师|医生|律师|会计|销售|运营|管理|创业|研究|分析师|自由职业)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "profession_identity".into(),
                min_confidence: 0.85,
            },
            // ── 技能水平 ──
            ExtractionRule {
                pattern: Regex::new(r"(擅长|精通|熟悉|会用|学过|在学|新手|刚入门|不太懂|不太会).{0,10}(Python|Rust|Java|JavaScript|前端|后端|算法|机器学习|数据分析|PS|设计|英语|日语)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "skill_level".into(),
                min_confidence: 0.80,
            },
            // ── 通用偏好 ──
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向|习惯)(用|做|看|吃|玩|听|写|选)").unwrap(),
                event_type: EventType::Preference,
                action: "general_preference".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(讨厌|不喜欢|烦|受不了|反感).{0,8}").unwrap(),
                event_type: EventType::Preference,
                action: "dislike".into(),
                min_confidence: 0.70,
            },
            // ── 情绪状态 ──
            ExtractionRule {
                pattern: Regex::new(r"(开心|高兴|兴奋|激动|爽|满足|愉快|欣慰)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "positive_mood".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(沮丧|失落|难过|伤心|绝望|崩溃|抑郁|郁闷|烦躁)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "negative_mood".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(焦虑|担心|害怕|紧张|不安|压力大|累|疲惫)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "stressed".into(),
                min_confidence: 0.70,
            },
            // ── 需求目标 ──
            ExtractionRule {
                pattern: Regex::new(r"(希望|想要|需要|打算|计划|准备|目标是).{0,15}(学|做|买|换|去|找|完成|实现)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "goal_expressed".into(),
                min_confidence: 0.75,
            },
            // ── 工作方式 ──
            ExtractionRule {
                pattern: Regex::new(r"(习惯|通常|一般|总是|经常).{0,8}(早上|晚上|深夜|周末|远程|在家|在公司)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "work_habit".into(),
                min_confidence: 0.70,
            },
            // ── 知识领域 ──
            ExtractionRule {
                pattern: Regex::new(r"(了解|知道|研究|关注|涉猎|专注).{0,6}(AI|人工智能|区块链|金融|量化|嵌入式|云计算|安全|前端|后端|运维|设计)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "knowledge_domain".into(),
                min_confidence: 0.75,
            },
            // ── 沟通偏好 ──
            ExtractionRule {
                pattern: Regex::new(r"(请|麻烦|帮我).{0,4}(简洁|详细|用中文|用英文|举个例子|不要太长|一步一步)").unwrap(),
                event_type: EventType::Preference,
                action: "communication_style".into(),
                min_confidence: 0.75,
            },
            // ── Coding 偏好 ──
            ExtractionRule {
                pattern: Regex::new(r"(用|写|改成|换成|迁移到).{0,6}(React|Vue|Angular|Svelte|Next|Nuxt|Express|FastAPI|Django|Spring|Rust|Go|TypeScript|Python|Java|C\+\+|Swift)").unwrap(),
                event_type: EventType::Preference,
                action: "tech_stack_preference".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(不要|别|不用).{0,4}(注释|comment|文档|docstring|type hint|any|console\.log)").unwrap(),
                event_type: EventType::Preference,
                action: "code_style_dislike".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(用|加上|要有|保持).{0,4}(TDD|测试|unit test|type safe|严格模式|eslint|prettier|fmt|clippy)").unwrap(),
                event_type: EventType::Preference,
                action: "code_quality_preference".into(),
                min_confidence: 0.75,
            },
            ExtractionRule {
                pattern: Regex::new(r"(这个|我的|当前).{0,4}(项目|工程|仓库|repo).{0,6}(是|用的|基于)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "project_context".into(),
                min_confidence: 0.80,
            },
        ];
        Self::new(rules)
    }

    pub fn extract(&self, text: &str, context: &str) -> Vec<Event> {
        let mut events = Vec::new();
        for rule in &self.rules {
            if rule.pattern.is_match(text) {
                events.push(Event::new(
                    rule.event_type.clone(),
                    rule.action.clone(),
                    context.to_string(),
                    rule.min_confidence,
                    text.to_string(),
                ));
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;

    fn make_extractor() -> RuleBasedExtractor {
        RuleBasedExtractor::with_default_rules()
    }

    #[test]
    fn test_extract_interest() {
        let extractor = make_extractor();
        let text = "我很喜欢编程，尤其是Rust语言";
        let events = extractor.extract(text, "chat");
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.action == "interest_hobby"));
    }

    #[test]
    fn test_extract_profession() {
        let extractor = make_extractor();
        let text = "我是一名后端开发工程师，写了5年代码了";
        let events = extractor.extract(text, "chat");
        assert!(events.iter().any(|e| e.action == "profession_identity"));
    }

    #[test]
    fn test_extract_skill() {
        let extractor = make_extractor();
        let text = "我擅长Python和数据分析，但Rust我还是新手";
        let events = extractor.extract(text, "chat");
        assert!(events.iter().any(|e| e.action == "skill_level"));
    }

    #[test]
    fn test_extract_preference() {
        let extractor = make_extractor();
        let text = "我还是更喜欢用Vim写代码";
        let events = extractor.extract(text, "coding");
        let pref = events
            .iter()
            .find(|e| e.event_type == EventType::Preference);
        assert!(pref.is_some());
    }

    #[test]
    fn test_extract_mood() {
        let extractor = make_extractor();
        let text = "今天工作特别累，压力大到失眠";
        let events = extractor.extract(text, "mood");
        assert!(events.iter().any(|e| e.action == "stressed"));
    }

    #[test]
    fn test_no_false_positive() {
        let extractor = make_extractor();
        let text = "今天天气不错，我去公园散了会步";
        let events = extractor.extract(text, "casual");
        assert!(events.is_empty());
    }

    #[test]
    fn test_extract_goal() {
        let extractor = make_extractor();
        let text = "我计划下个月去学习机器学习";
        let events = extractor.extract(text, "chat");
        assert!(events.iter().any(|e| e.action == "goal_expressed"));
    }

    #[test]
    fn test_extract_communication_style() {
        let extractor = make_extractor();
        let text = "请帮我用中文解释一下这个概念";
        let events = extractor.extract(text, "chat");
        assert!(events.iter().any(|e| e.action == "communication_style"));
    }
}
