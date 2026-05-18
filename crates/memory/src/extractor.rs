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
            // 行为模式
            ExtractionRule {
                pattern: Regex::new(r"(亏|跌|赔).*(加仓|补仓|买入|抄底)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "loss_chase".into(),
                min_confidence: 0.75,
            },
            ExtractionRule {
                pattern: Regex::new(r"(追高|追涨|涨停.*买)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "chase_high".into(),
                min_confidence: 0.75,
            },
            ExtractionRule {
                pattern: Regex::new(r"(不止损|舍不得割|扛着|死扛)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "avoid_stop_loss".into(),
                min_confidence: 0.70,
            },
            // 偏好
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(短线|快进快出|日内)").unwrap(),
                event_type: EventType::Preference,
                action: "prefers_short_term".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(长线|价值投资|长期持有)").unwrap(),
                event_type: EventType::Preference,
                action: "prefers_long_term".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(科技股|成长股|AI|芯片|新能源)")
                    .unwrap(),
                event_type: EventType::Preference,
                action: "prefers_tech_stocks".into(),
                min_confidence: 0.80,
            },
            // 通用偏好
            ExtractionRule {
                pattern: Regex::new(r"还是更?(喜欢|习惯|倾向)(用|做|看)").unwrap(),
                event_type: EventType::Preference,
                action: "general_preference".into(),
                min_confidence: 0.65,
            },
            // 情绪
            ExtractionRule {
                pattern: Regex::new(r"(沮丧|失落|难过|伤心|绝望|崩溃)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "negative_emotional".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(开心|兴奋|激动|高兴|爽)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "positive_emotional".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(焦虑|担心|害怕|紧张|不安)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "anxious".into(),
                min_confidence: 0.70,
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
    fn test_extract_loss_chase_pattern() {
        let extractor = make_extractor();
        let text = "虽然已经亏了30%，但是我觉得还能涨回来，我又加仓了";
        let events = extractor.extract(text, "trading");
        assert!(!events.is_empty());
        let loss_chase = events.iter().find(|e| e.action == "loss_chase");
        assert!(loss_chase.is_some());
        assert!(loss_chase.unwrap().confidence >= 0.7);
    }

    #[test]
    fn test_extract_preference() {
        let extractor = make_extractor();
        let text = "我还是更喜欢用Python写代码，Java太啰嗦了";
        let events = extractor.extract(text, "coding");
        let pref = events
            .iter()
            .find(|e| e.event_type == EventType::Preference);
        assert!(pref.is_some());
    }

    #[test]
    fn test_no_false_positive() {
        let extractor = make_extractor();
        let text = "今天天气不错，我去公园散了会步";
        let events = extractor.extract(text, "casual");
        let patterns: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::BehaviorPattern)
            .collect();
        assert!(
            patterns.is_empty(),
            "casual chat should not produce behavior patterns"
        );
    }

    #[test]
    fn test_emotional_state_detection() {
        let extractor = make_extractor();
        let text = "我今天真的很沮丧，工作上一堆破事，感觉什么都做不好";
        let events = extractor.extract(text, "mood");
        let emotion = events
            .iter()
            .find(|e| e.event_type == EventType::EmotionalState);
        assert!(emotion.is_some());
    }
}
