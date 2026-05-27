//! Persona provider — assembles user profile from pre-queried cognition rows.
//! Sorts by confidence × evidence_count to surface the most reliable traits first.

use anyhow::Result;
use loom_types::PersonaProvider;

use crate::store::CognitionRow;

/// Builds a persona summary from a set of cognition rows.
pub struct CognitionsPersonaProvider {
    rows: Vec<CognitionRow>,
    top_n: usize,
}

impl CognitionsPersonaProvider {
    pub fn new(rows: Vec<CognitionRow>) -> Self {
        Self { rows, top_n: 10 }
    }

    pub fn with_top_n(mut self, n: usize) -> Self {
        self.top_n = n;
        self
    }
}

#[async_trait::async_trait]
impl PersonaProvider for CognitionsPersonaProvider {
    async fn summarize(&self) -> Result<String> {
        if self.rows.is_empty() {
            return Ok(String::new());
        }

        // Sort by confidence * evidence_count descending, take top_n
        let mut sorted: Vec<&CognitionRow> = self.rows.iter().collect();
        sorted.sort_by(|a, b| {
            let sa = a.confidence * (a.evidence_count as f64);
            let sb = b.confidence * (b.evidence_count as f64);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(self.top_n);

        let mut techs = Vec::new();
        let mut interests = Vec::new();
        let mut others = Vec::new();

        for r in &sorted {
            let entry = r.value.clone();
            if r.trait_name.starts_with("uses_") {
                techs.push(entry);
            } else if r.trait_name.starts_with("interest_") {
                interests.push(entry);
            } else {
                others.push(format!("{}={}", r.trait_name, r.value));
            }
        }

        let mut parts = Vec::new();
        if !techs.is_empty() {
            techs.sort();
            techs.dedup();
            parts.push(format!("Uses: {}", techs.join(", ")));
        }
        if !interests.is_empty() {
            interests.sort();
            interests.dedup();
            parts.push(format!("Interests: {}", interests.join(", ")));
        }
        if !others.is_empty() {
            others.dedup();
            parts.push(format!("Context: {}", others.join("; ")));
        }

        Ok(format!("User profile — {}", parts.join(" | ")))
    }

    fn invalidate(&self) {}
}
