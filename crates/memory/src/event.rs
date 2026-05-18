use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 用户重复出现的行为模式
    BehaviorPattern,
    /// 用户明确表达的偏好
    Preference,
    /// 用户陈述的事实信息
    Fact,
    /// 用户与AI的关系变化
    Relationship,
    /// 用户传达的情绪状态
    EmotionalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub action: String,
    pub context: String,
    pub confidence: f64,
    pub source_session: Option<String>,
    /// 原始对话中触发此事件的文本片段
    pub source_text: String,
    pub payload: Option<serde_json::Value>,
}

impl Event {
    pub fn new(
        event_type: EventType,
        action: impl Into<String>,
        context: impl Into<String>,
        confidence: f64,
        source_text: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            timestamp: Utc::now(),
            event_type,
            action: action.into(),
            context: context.into(),
            confidence,
            source_session: None,
            source_text: source_text.into(),
            payload: None,
        }
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.source_session = Some(session_id.into());
        self
    }

    pub fn event_type_as_str(&self) -> &str {
        match self.event_type {
            EventType::BehaviorPattern => "behavior_pattern",
            EventType::Preference => "preference",
            EventType::Fact => "fact",
            EventType::Relationship => "relationship",
            EventType::EmotionalState => "emotional_state",
        }
    }

    pub fn event_type_from_str(s: &str) -> EventType {
        match s {
            "behavior_pattern" => EventType::BehaviorPattern,
            "preference" => EventType::Preference,
            "fact" => EventType::Fact,
            "relationship" => EventType::Relationship,
            "emotional_state" => EventType::EmotionalState,
            _ => EventType::Fact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            EventType::BehaviorPattern,
            "loss_chase",
            "trading",
            0.87,
            "我又加仓了，虽然已经亏了很多",
        );
        assert_eq!(event.action, "loss_chase");
        assert_eq!(event.confidence, 0.87);
        assert!(event.id.is_none());
    }

    #[test]
    fn test_event_json_roundtrip() {
        let event = Event::new(
            EventType::Preference,
            "prefers_short_term",
            "trading_style",
            0.95,
            "我喜欢快进快出",
        );
        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.action, "prefers_short_term");
    }

    #[test]
    fn test_with_payload() {
        let event = Event::new(
            EventType::Fact,
            "owns_stock",
            "portfolio",
            1.0,
            "我持有AAPL",
        )
        .with_payload(serde_json::json!({"symbol": "AAPL", "shares": 100}));
        assert!(event.payload.is_some());
    }

    #[test]
    fn test_with_session() {
        let event = Event::new(
            EventType::EmotionalState,
            "anxious",
            "trading",
            0.85,
            "我好焦虑",
        )
        .with_session("session_42");
        assert_eq!(event.source_session, Some("session_42".to_string()));
    }
}
