//! Persona provider — assembles a rich structured user profile from cognition rows,
//! knowledge graph nodes, and session metadata.
//!
//! The `RichPersonaProvider` queries the cognition store to derive tech proficiencies,
//! preferences, goals, working style, and communication style. It then formats the
//! structured persona into a ~500-token prompt-ready text block.
//!
//! Backward-compatible: implements the existing `PersonaProvider` trait so existing
//! callers can migrate transparently.

use anyhow::Result;
use chrono::Utc;
use loom_types::PersonaProvider;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::store::CognitionStore;

// ============================================================================
// Rich persona data types
// ============================================================================

/// Proficiency level inferred from evidence count for a technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProficiencyLevel {
    /// 1-2 evidence events
    Beginner,
    /// 3-5 evidence events
    Intermediate,
    /// 6-10 evidence events
    Advanced,
    /// 11+ evidence events
    Expert,
}

impl ProficiencyLevel {
    /// Derive a proficiency level from an evidence count.
    pub fn from_evidence_count(n: i64) -> Self {
        match n {
            0..=2 => Self::Beginner,
            3..=5 => Self::Intermediate,
            6..=10 => Self::Advanced,
            _ => Self::Expert,
        }
    }

    /// Human-readable label for prompt formatting.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Beginner => "beginner",
            Self::Intermediate => "intermediate",
            Self::Advanced => "advanced",
            Self::Expert => "expert",
        }
    }
}

/// A technology or tool the user has demonstrated proficiency with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechProficiency {
    /// Technology name (e.g. "Rust", "React", "Docker").
    pub name: String,
    /// Inferred proficiency level.
    pub level: ProficiencyLevel,
    /// Confidence in this inference, 0.0-1.0.
    pub confidence: f64,
    /// Number of distinct evidence events observed.
    pub evidence_count: i64,
}

/// A stated or inferred user preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preference {
    /// Domain key (e.g. "editor", "color_scheme", "workflow").
    pub key: String,
    /// Preferred value (e.g. "VS Code", "dark", "TDD").
    pub value: String,
    /// How strongly this preference is held, 0.0-1.0.
    pub strength: f64,
    /// Raw evidence strings that led to this inference.
    pub evidence: Vec<String>,
}

/// Status of a tracked goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    /// Goal is actively being pursued.
    Active,
    /// Goal has been completed.
    Achieved,
    /// Goal was abandoned or superseded.
    Abandoned,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Achieved => "achieved",
            Self::Abandoned => "abandoned",
        }
    }
}

/// A user goal extracted from cognition patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Free-text description of the goal.
    pub description: String,
    /// Current status.
    pub status: GoalStatus,
    /// Priority (higher = more important). Range 1-10.
    pub priority: i32,
}

/// High-level working approach preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Approach {
    /// Prefers to see code first, then discuss.
    CodeFirst,
    /// Prefers to plan architecture before writing code.
    PlanFirst,
    /// Prefers back-and-forth conversation before implementation.
    Conversational,
}

impl Approach {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CodeFirst => "code-first",
            Self::PlanFirst => "plan-first",
            Self::Conversational => "conversational",
        }
    }
}

/// Desired verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verbosity {
    /// Short, direct answers.
    Concise,
    /// Moderate detail.
    Balanced,
    /// Full explanations with context.
    Detailed,
}

impl Verbosity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Concise => "concise",
            Self::Balanced => "balanced",
            Self::Detailed => "detailed",
        }
    }
}

/// Working style preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingStyle {
    pub approach: Approach,
    pub verbosity: Verbosity,
}

impl Default for WorkingStyle {
    fn default() -> Self {
        Self {
            approach: Approach::Conversational,
            verbosity: Verbosity::Balanced,
        }
    }
}

/// Formality level in communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Formality {
    Casual,
    Neutral,
    Formal,
}

impl Formality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Casual => "casual",
            Self::Neutral => "neutral",
            Self::Formal => "formal",
        }
    }
}

/// Communication style preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    /// Preferred language (e.g. "zh-CN", "en-US").
    pub language: String,
    /// Preferred formality.
    pub formality: Formality,
}

impl Default for CommunicationStyle {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            formality: Formality::Neutral,
        }
    }
}

/// The assembled rich persona — a structured representation of everything
/// the system knows about a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichPersona {
    /// Technologies the user knows, with inferred proficiency levels.
    pub tech_stack: Vec<TechProficiency>,
    /// Stated or inferred preferences.
    pub preferences: Vec<Preference>,
    /// Tracked goals.
    pub goals: Vec<Goal>,
    /// Working style preferences.
    pub working_style: WorkingStyle,
    /// Communication preferences.
    pub communication: CommunicationStyle,
    /// High-level expertise areas (e.g. "backend", "DevOps", "security").
    pub expertise_areas: Vec<String>,
    /// Recurring behavioural patterns (e.g. "prefers kanban", "late-night coder").
    pub behavioural_patterns: Vec<String>,
    /// ISO-8601 timestamp of the last assembly.
    pub last_updated: String,
}

impl RichPersona {
    /// Create a minimal empty persona with sensible defaults.
    pub fn empty() -> Self {
        Self {
            tech_stack: Vec::new(),
            preferences: Vec::new(),
            goals: Vec::new(),
            working_style: WorkingStyle::default(),
            communication: CommunicationStyle::default(),
            expertise_areas: Vec::new(),
            behavioural_patterns: Vec::new(),
            last_updated: Utc::now().to_rfc3339(),
        }
    }
}

// ============================================================================
// Free assembly functions — operate on a Connection reference directly
// ============================================================================

/// Build tech_stack from `uses_*` cognitions and kg_nodes of type "Technology".
fn assemble_tech_stack(conn: &Connection) -> Result<Vec<TechProficiency>> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 200, 0)?;

    let mut tech_map: HashMap<String, (f64, i64)> = HashMap::new();

    for row in &rows {
        if row.trait_name.starts_with("uses_") {
            let name = row.value.clone();
            let entry = tech_map.entry(name).or_insert((0.0, 0));
            entry.0 = entry.0.max(row.confidence);
            entry.1 += row.evidence_count as i64;
        }
    }

    // Also pull Technology nodes from the knowledge graph as supporting evidence
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name, confidence, COALESCE(evidence_count, 1) FROM kg_nodes
         WHERE entity_type = 'Technology' ORDER BY evidence_count DESC LIMIT 50",
    ) && let Ok(kg_rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, f64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    }) {
        for kg_row in kg_rows.flatten() {
            let (name, confidence, evidence_count) = kg_row;
            let entry = tech_map.entry(name).or_insert((0.0, 0));
            entry.0 = entry.0.max(confidence);
            entry.1 += evidence_count;
        }
    }

    let mut techs: Vec<TechProficiency> = tech_map
        .into_iter()
        .map(|(name, (confidence, evidence_count))| TechProficiency {
            level: ProficiencyLevel::from_evidence_count(evidence_count),
            name,
            confidence: confidence.clamp(0.0, 1.0),
            evidence_count,
        })
        .collect();

    // Sort descending by evidence_count then confidence
    techs.sort_by(|a, b| {
        b.evidence_count.cmp(&a.evidence_count).then_with(|| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    Ok(techs)
}

/// Extract preferences from Chinese-pattern cognitions (我喜欢/我习惯/我讨厌)
/// and English equivalents (prefers_*, dislikes_*).
fn assemble_preferences(conn: &Connection) -> Result<Vec<Preference>> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 200, 0)?;

    let mut prefs: Vec<Preference> = Vec::new();

    for row in &rows {
        let tn = &row.trait_name;
        let combined = format!("{} {}", tn, row.value);
        let combined_lower = combined.to_lowercase();

        // Exact trait name prefixes
        if tn.starts_with("喜欢_") || tn.starts_with("习惯_") {
            prefs.push(Preference {
                key: "preference".into(),
                value: row.value.clone(),
                strength: (row.confidence * 0.9).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        } else if tn.starts_with("讨厌_") || tn.starts_with("不喜欢_") {
            prefs.push(Preference {
                key: "dislike".into(),
                value: row.value.clone(),
                strength: (row.confidence * 0.9).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        } else if tn.starts_with("prefers_") {
            prefs.push(Preference {
                key: tn.replacen("prefers_", "", 1),
                value: row.value.clone(),
                strength: (row.confidence * 0.85).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        } else if tn.starts_with("dislikes_") {
            prefs.push(Preference {
                key: format!("dislike_{}", tn.replacen("dislikes_", "", 1)),
                value: row.value.clone(),
                strength: (row.confidence * 0.85).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        }
        // Broad keyword matching in trait_name + value combined
        else if combined_lower.contains("偏好")
            || combined_lower.contains("喜欢")
            || combined_lower.contains("preference")
            || combined_lower.contains("prefer")
        {
            prefs.push(Preference {
                key: "preference".into(),
                value: row.value.clone(),
                strength: (row.confidence * 0.7).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        } else if combined_lower.contains("讨厌")
            || combined_lower.contains("不喜欢")
            || combined_lower.contains("dislike")
        {
            prefs.push(Preference {
                key: "dislike".into(),
                value: row.value.clone(),
                strength: (row.confidence * 0.7).clamp(0.0, 1.0),
                evidence: vec![format!("cognition#{}", row.id)],
            });
        }
    }

    // Deduplicate: keep the highest-strength entry for each (key, value) pair
    prefs.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen: HashSet<(String, String)> = HashSet::new();
    prefs.retain(|p| seen.insert((p.key.clone(), p.value.clone())));

    Ok(prefs)
}

/// Extract goals from Chinese-pattern cognitions (我想/我需要/我计划/目标_)
/// and English equivalents (goal_*).
fn assemble_goals(conn: &Connection) -> Result<Vec<Goal>> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 200, 0)?;

    let mut goals: Vec<Goal> = Vec::new();

    for row in &rows {
        let tn = &row.trait_name;
        let combined = format!("{} {}", tn, row.value);
        let combined_lower = combined.to_lowercase();

        let is_goal = tn.starts_with("想_")
            || tn.starts_with("需要_")
            || tn.starts_with("计划_")
            || tn.starts_with("目标_")
            || tn.starts_with("goal_")
            || tn.starts_with("intent_")
            || tn.starts_with("working_on_")
            || combined_lower.contains("目标")
            || combined_lower.contains("想实现")
            || combined_lower.contains("打算")
            || combined_lower.contains("计划")
            || combined_lower.contains("需要")
            || combined_lower.contains("goal")
            || combined_lower.contains("want to")
            || combined_lower.contains("plan to")
            || combined_lower.contains("working on");

        if is_goal {
            let status = if row.value.contains("完成") || row.value.contains("done") {
                GoalStatus::Achieved
            } else if row.value.contains("放弃") || row.value.contains("abandoned") {
                GoalStatus::Abandoned
            } else {
                GoalStatus::Active
            };

            let priority = ((row.confidence * 10.0).round() as i32).clamp(1, 10);

            goals.push(Goal {
                description: row.value.clone(),
                status,
                priority,
            });
        }
    }

    // Sort by priority desc, active first
    goals.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.status.as_str().cmp(b.status.as_str()))
    });

    Ok(goals)
}

/// Infer working style from conversation-pattern cognitions.
fn assemble_working_style(conn: &Connection) -> Result<WorkingStyle> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 50, 0)?;

    let mut code_first = 0u32;
    let mut plan_first = 0u32;
    let mut conversational = 0u32;
    let mut concise = 0u32;
    let mut balanced = 0u32;
    let mut detailed = 0u32;

    for row in &rows {
        let combined = format!("{} {}", row.trait_name, row.value);
        let combined_lower = combined.to_lowercase();

        // Approach signals
        if combined_lower.contains("直接写")
            || combined_lower.contains("代码")
            || combined_lower.contains("code_first")
        {
            code_first += 1;
        }
        if combined_lower.contains("计划")
            || combined_lower.contains("设计")
            || combined_lower.contains("plan_first")
            || combined_lower.contains("架构")
        {
            plan_first += 1;
        }
        if combined_lower.contains("讨论")
            || combined_lower.contains("对话")
            || combined_lower.contains("conversational")
        {
            conversational += 1;
        }

        // Verbosity signals
        if combined_lower.contains("简洁")
            || combined_lower.contains("concise")
            || combined_lower.contains("简短")
        {
            concise += 1;
        }
        if combined_lower.contains("详细")
            || combined_lower.contains("detailed")
            || combined_lower.contains("详尽")
        {
            detailed += 1;
        }
    }

    // balanced tracks real "Balanced" evidence from cognitions, not a forced bias.
    if balanced == 0 && concise == 0 && detailed == 0 {
        // No verbosity signal at all — default to Balanced.
        balanced = 1;
    }

    let approach = if code_first > plan_first && code_first > conversational {
        Approach::CodeFirst
    } else if plan_first > code_first && plan_first > conversational {
        Approach::PlanFirst
    } else {
        Approach::Conversational
    };

    let verbosity = if concise > detailed && concise > balanced {
        Verbosity::Concise
    } else if detailed > concise && detailed > balanced {
        Verbosity::Detailed
    } else {
        Verbosity::Balanced
    };

    Ok(WorkingStyle {
        approach,
        verbosity,
    })
}

/// Infer communication style (language + formality) from session metadata
/// and language-related cognitions.
fn assemble_communication(conn: &Connection) -> Result<CommunicationStyle> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 50, 0)?;

    let mut lang_signal = String::new();
    let mut casual = 0u32;
    let mut neutral = 0u32;
    let mut formal = 0u32;

    for row in &rows {
        // Language detection
        if row.trait_name == "language" || row.trait_name.starts_with("lang_") {
            lang_signal = row.value.clone();
        }

        // Formality signals
        let combined_lower = format!("{} {}", row.trait_name, row.value).to_lowercase();
        if combined_lower.contains("随意")
            || combined_lower.contains("casual")
            || combined_lower.contains("轻松")
        {
            casual += 1;
        } else if combined_lower.contains("正式")
            || combined_lower.contains("formal")
            || combined_lower.contains("专业")
        {
            formal += 1;
        } else {
            neutral += 1;
        }
    }

    // Try session metadata for language hint
    if lang_signal.is_empty()
        && let Ok(mut stmt) =
            conn.prepare("SELECT metadata FROM sessions WHERE metadata IS NOT NULL LIMIT 5")
        && let Ok(meta_rows) = stmt.query_map([], |r| r.get::<_, String>(0))
    {
        for meta_row in meta_rows.flatten() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&meta_row)
                && let Some(lang) = v.get("language").and_then(|l| l.as_str())
            {
                lang_signal = lang.to_string();
                break;
            }
        }
    }

    let language = if lang_signal.is_empty() {
        "zh-CN".to_string()
    } else {
        lang_signal
    };

    let formality = if formal > casual && formal > neutral / 2 {
        Formality::Formal
    } else if casual > formal && casual > neutral / 2 {
        Formality::Casual
    } else {
        Formality::Neutral
    };

    Ok(CommunicationStyle {
        language,
        formality,
    })
}

/// Derive expertise areas from kg_nodes content — Technology + Topic nodes,
/// with inference from node names. entity_type labels (Concept/Person/etc.)
/// are NOT meaningful as expertise areas.
fn assemble_expertise_areas(conn: &Connection) -> Result<Vec<String>> {
    let mut areas: Vec<String> = Vec::new();

    // Primary: collect high-frequency Technology/Tool/Language/Framework nodes by name.
    // These are the most reliable signal for what the user actually works with.
    let mut tech_names: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM kg_nodes
         WHERE entity_type IN ('Technology','Tool','Language','Framework','Library')
         AND scope != 'test'
         ORDER BY COALESCE(evidence_count, 1) DESC LIMIT 30",
    ) && let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0))
    {
        tech_names = rows.flatten().collect();
    }

    // Secondary: Topic nodes with high evidence (subject matter expertise)
    let mut topic_names: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM kg_nodes
         WHERE entity_type IN ('Topic','Concept')
         AND scope != 'test'
         ORDER BY COALESCE(evidence_count, 1) DESC LIMIT 10",
    ) && let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0))
    {
        topic_names = rows.flatten().collect();
    }

    let tech_strs: Vec<&str> = tech_names.iter().map(|s| s.as_str()).collect();

    // Infer broad domains from tech node names
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("rust") || l.contains(" go") || l.contains("python")
            || l.contains("java") || l.contains("c++") || l.contains("c#")
            || l.contains("ruby") || l.contains("swift") || l.contains("kotlin")
    }) {
        areas.push("backend".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("react") || l.contains("vue") || l.contains("angular")
            || l.contains("css") || l.contains("typescript") || l.contains("javascript")
            || l.contains("html") || l.contains("svelte") || l.contains("next")
    }) {
        areas.push("frontend".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("docker") || l.contains("kubernetes") || l.contains("k8s")
            || l.contains("nginx") || l.contains("terraform") || l.contains("ansible")
            || l.contains("ci/cd") || l.contains("github action")
    }) {
        areas.push("DevOps".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("ai") || l.contains("ml") || l.contains("tensor")
            || l.contains("pytorch") || l.contains("llm") || l.contains("gpt")
            || l.contains("hugging") || l.contains("model")
    }) {
        areas.push("AI/ML".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("sql") || l.contains("postgres") || l.contains("mysql")
            || l.contains("redis") || l.contains("mongo") || l.contains("sqlite")
            || l.contains("database") || l.contains("influx")
    }) {
        areas.push("databases".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("android") || l.contains("ios") || l.contains("flutter")
            || l.contains("react native") || l.contains("swift") || l.contains("kotlin")
    }) {
        areas.push("mobile".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("aws") || l.contains("azure") || l.contains("gcp")
            || l.contains("cloud") || l.contains("lambda") || l.contains("s3")
    }) {
        areas.push("cloud".into());
    }
    if tech_strs.iter().any(|n| {
        let l = n.to_lowercase();
        l.contains("security") || l.contains("auth") || l.contains("oauth")
            || l.contains("jwt") || l.contains("ssl") || l.contains("tls")
    }) {
        areas.push("security".into());
    }

    // Append high-signal topic names directly (capitalised, de-duplicated)
    for topic in topic_names.iter().take(5) {
        let t = topic.trim().to_string();
        if !t.is_empty() && !areas.iter().any(|a| a.to_lowercase() == t.to_lowercase()) {
            areas.push(t);
        }
    }

    if areas.is_empty() {
        areas.push("general".into());
    }

    Ok(areas)
}

/// Surface recurring behavioural patterns from cognitions.
fn assemble_behavioural_patterns(conn: &Connection) -> Result<Vec<String>> {
    let cognition = CognitionStore::new(conn);
    let rows = cognition.query_by_subject("USER", None, 100, 0)?;

    let mut patterns: Vec<String> = Vec::new();

    for row in &rows {
        let combined = format!("{} {}", row.trait_name, row.value);
        let combined_lower = combined.to_lowercase();

        if combined_lower.contains("深夜") || combined_lower.contains("late_night") {
            patterns.push("late-night-coder".into());
        }
        if combined_lower.contains("看板") || combined_lower.contains("kanban") {
            patterns.push("prefers-kanban".into());
        }
        if combined_lower.contains("tdd") || combined_lower.contains("测试驱动") {
            patterns.push("tdd-practitioner".into());
        }
        if combined_lower.contains("重构") || combined_lower.contains("refactor") {
            patterns.push("refactoring-minded".into());
        }
        if combined_lower.contains("文档") || combined_lower.contains("documentation") {
            patterns.push("documentation-oriented".into());
        }
        if combined_lower.contains("开源") || combined_lower.contains("open_source") {
            patterns.push("open-source-contributor".into());
        }
        if combined_lower.contains("review") || combined_lower.contains("审查") {
            patterns.push("code-review-advocate".into());
        }
    }

    patterns.sort();
    patterns.dedup();
    Ok(patterns)
}

// ============================================================================
// RichPersonaProvider — owns the assembled persona (Send + Sync)
// ============================================================================

/// A ready-to-use persona provider that owns a fully assembled `RichPersona`.
///
/// Call `RichPersonaProvider::assemble(conn)` to build one from a database
/// connection, or construct directly from an existing `RichPersona`.
///
/// Implements the `PersonaProvider` trait for backward compatibility:
/// `summarize()` returns the `format_for_prompt()` text block.
pub struct RichPersonaProvider {
    persona: RichPersona,
}

impl RichPersonaProvider {
    /// Assemble a `RichPersona` from the database and wrap it in a provider.
    ///
    /// Queries the cognition store, knowledge graph, and session metadata
    /// to derive a complete user profile.
    pub fn assemble(conn: &Connection) -> Result<Self> {
        let persona = RichPersona {
            tech_stack: assemble_tech_stack(conn)?,
            preferences: assemble_preferences(conn)?,
            goals: assemble_goals(conn)?,
            working_style: assemble_working_style(conn)?,
            communication: assemble_communication(conn)?,
            expertise_areas: assemble_expertise_areas(conn)?,
            behavioural_patterns: assemble_behavioural_patterns(conn)?,
            last_updated: Utc::now().to_rfc3339(),
        };
        Ok(Self { persona })
    }

    /// Create a provider from an already-assembled `RichPersona`.
    pub fn from_persona(persona: RichPersona) -> Self {
        Self { persona }
    }

    /// Borrow the underlying `RichPersona`.
    pub fn persona(&self) -> &RichPersona {
        &self.persona
    }

    /// Consume the provider and return the inner `RichPersona`.
    pub fn into_persona(self) -> RichPersona {
        self.persona
    }

    /// Format a `RichPersona` into a compact, readable text block suitable for
    /// inclusion in a system prompt. Targets roughly 500 tokens.
    pub fn format_for_prompt(persona: &RichPersona) -> String {
        let mut lines: Vec<String> = Vec::new();

        lines.push("[USER PERSONA]".into());

        // --- Tech stack ---
        if !persona.tech_stack.is_empty() {
            lines.push(String::new());
            let techs: Vec<String> = persona
                .tech_stack
                .iter()
                .take(12)
                .map(|t| format!("{} ({})", t.name, t.level.as_str()))
                .collect();
            lines.push(format!("Tech stack: {}", techs.join(", ")));
        }

        // --- Expertise areas ---
        if !persona.expertise_areas.is_empty() {
            lines.push(format!("Expertise: {}", persona.expertise_areas.join(", ")));
        }

        // --- Preferences ---
        if !persona.preferences.is_empty() {
            lines.push(String::new());
            let likes: Vec<&Preference> = persona
                .preferences
                .iter()
                .filter(|p| p.key == "preference" || !p.key.starts_with("dislike"))
                .collect();
            let dislikes: Vec<&Preference> = persona
                .preferences
                .iter()
                .filter(|p| p.key.starts_with("dislike"))
                .collect();

            if !likes.is_empty() {
                let vals: Vec<&str> = likes.iter().take(8).map(|p| p.value.as_str()).collect();
                lines.push(format!("Likes: {}", vals.join(", ")));
            }
            if !dislikes.is_empty() {
                let vals: Vec<&str> = dislikes.iter().take(5).map(|p| p.value.as_str()).collect();
                lines.push(format!("Dislikes: {}", vals.join(", ")));
            }
        }

        // --- Goals ---
        let active_goals: Vec<&Goal> = persona
            .goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active)
            .take(5)
            .collect();
        if !active_goals.is_empty() {
            lines.push(String::new());
            let goal_strs: Vec<String> = active_goals
                .iter()
                .map(|g| format!("{} [p{}]", g.description, g.priority))
                .collect();
            lines.push(format!("Active goals: {}", goal_strs.join("; ")));
        }

        // --- Working style ---
        lines.push(String::new());
        lines.push(format!(
            "Working style: {} approach, {} responses",
            persona.working_style.approach.as_str(),
            persona.working_style.verbosity.as_str()
        ));

        // --- Communication ---
        lines.push(format!(
            "Communication: {} ({})",
            persona.communication.language,
            persona.communication.formality.as_str()
        ));

        // --- Behavioural patterns ---
        if !persona.behavioural_patterns.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Patterns: {}",
                persona.behavioural_patterns.join(", ")
            ));
        }

        // --- Timestamp ---
        lines.push(String::new());
        lines.push(format!("Last updated: {}", persona.last_updated));

        lines.join("\n")
    }

    /// Alias for `format_for_prompt` at the instance level.
    pub fn to_prompt(&self) -> String {
        Self::format_for_prompt(&self.persona)
    }
}

// ============================================================================
// PersonaProvider trait impl (backward-compatible)
// ============================================================================

#[async_trait::async_trait]
impl PersonaProvider for RichPersonaProvider {
    async fn summarize(&self) -> Result<String> {
        Ok(Self::format_for_prompt(&self.persona))
    }

    fn invalidate(&self) {
        // The provider owns its assembled persona. To refresh, create a new
        // provider with RichPersonaProvider::assemble(conn).
    }
}
