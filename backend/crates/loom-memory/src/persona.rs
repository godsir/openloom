//! Persona provider — assembles user profile from pre-queried cognition rows.

use anyhow::Result;
use loom_types::PersonaProvider;

use crate::store::CognitionRow;

/// Builds a persona summary from a set of cognition rows.
pub struct CognitionsPersonaProvider {
    rows: Vec<CognitionRow>,
}

impl CognitionsPersonaProvider {
    pub fn new(rows: Vec<CognitionRow>) -> Self {
        Self { rows }
    }
}

#[async_trait::async_trait]
impl PersonaProvider for CognitionsPersonaProvider {
    async fn summarize(&self) -> Result<String> {
        if self.rows.is_empty() { return Ok(String::new()); }

        let mut techs = Vec::new();
        let mut interests = Vec::new();
        let mut others = Vec::new();

        for r in &self.rows {
            let entry = r.value.clone();
            if r.trait_name.starts_with("uses_") { techs.push(entry); }
            else if r.trait_name.starts_with("interest_") { interests.push(entry); }
            else { others.push(format!("{}={}", r.trait_name, r.value)); }
        }

        let mut parts = Vec::new();
        if !techs.is_empty() { techs.sort(); techs.dedup(); parts.push(format!("Uses: {}", techs.join(", "))); }
        if !interests.is_empty() { interests.sort(); interests.dedup(); parts.push(format!("Interests: {}", interests.join(", "))); }
        if !others.is_empty() { others.dedup(); parts.push(format!("Context: {}", others.join("; "))); }

        Ok(format!("User profile — {}", parts.join(" | ")))
    }

    fn invalidate(&self) {}
}
