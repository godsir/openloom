//! 专家团（Expert Team）配置类型。

use serde::{Deserialize, Serialize};

/// 协作策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TeamStrategy {
    /// 合成模式：各成员并行回答 → 团长综合结论
    #[default]
    Synthesize,
    /// 辩论模式：两轮互相质疑后综合结论
    Debate,
}

/// 团长配置（负责汇总/协调）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamCaptain {
    /// 团长模型，None 表示跟随全局
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 团长 system prompt 覆写
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_override: Option<String>,
}

/// 成员来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MemberSource {
    /// 内联定义：直接在团队表单中填写配置
    Inline {
        persona: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
    },
    /// 引用已有 Agent 的 config name
    AgentRef(String),
}

/// 团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub source: MemberSource,
}

/// 专家团配置，与 AgentConfig 同级存储。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// 唯一标识（UUID v7）
    pub id: String,
    /// 团队名称，如"代码审查团"
    pub name: String,
    /// 用途说明
    #[serde(default)]
    pub description: String,
    /// 协作策略
    #[serde(default)]
    pub strategy: TeamStrategy,
    /// 团长配置
    #[serde(default)]
    pub captain: TeamCaptain,
    /// 成员列表
    #[serde(default)]
    pub members: Vec<TeamMember>,
}
