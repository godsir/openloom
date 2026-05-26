//! Entity extraction from conversation text.
//!
//! Supports rule-based (regex) and LLM-based extraction strategies.
//! Extracted entities and relationships feed into the knowledge graph.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// An entity discovered in conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String, // Person | Technology | Project | Concept | Tool | Topic | Organization
    pub description: String,
    pub confidence: f64,
    pub aliases: Vec<String>,
    pub scope: String,
}

/// A relationship discovered between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source_name: String,
    pub target_name: String,
    pub relation_type: String, // uses | works_on | knows | interested_in | dislikes | depends_on | part_of
    pub fact: String,          // human-readable: "USER uses Rust for backend development"
    pub confidence: f64,
    pub scope: String,
}

/// Trait for entity+relationship extraction from text.
pub trait EntityExtractor: Send + Sync {
    fn extract_entities(&self, text: &str, context: &str) -> Result<Vec<ExtractedEntity>>;
    fn extract_relationships(&self, text: &str, entities: &[ExtractedEntity]) -> Result<Vec<ExtractedRelationship>>;
}

/// Rule-based extractor using keyword matching and simple patterns.
pub struct RuleBasedEntityExtractor;

impl EntityExtractor for RuleBasedEntityExtractor {
    fn extract_entities(&self, text: &str, _context: &str) -> Result<Vec<ExtractedEntity>> {
        let mut entities = Vec::new();
        let lower = text.to_lowercase();

        // Technology detection via common keywords
        let techs = [
            ("rust", "Technology", "Systems programming language"),
            ("python", "Technology", "General-purpose programming language"),
            ("typescript", "Technology", "Typed JavaScript superset"),
            ("golang", "Technology", "Go programming language"),
            ("docker", "Technology", "Container platform"),
            ("kubernetes", "Technology", "Container orchestration"),
            ("react", "Technology", "UI framework"),
            ("sqlite", "Technology", "Embedded database"),
            ("openloom", "Project", "Personal AI kernel"),
            ("mcp", "Concept", "Model Context Protocol"),
            ("lsp", "Concept", "Language Server Protocol"),
            ("agent", "Concept", "AI agent system"),
            ("skill", "Concept", "Skill/plugin system"),
            ("雷火", "Organization", "Leihuo / NetEase"),
        ];
        for (kw, etype, desc) in &techs {
            if lower.contains(kw) {
                entities.push(ExtractedEntity {
                    name: kw.to_string(), entity_type: etype.to_string(),
                    description: desc.to_string(), confidence: 0.6, aliases: vec![],
                    scope: "global".into(),
                });
            }
        }
        Ok(entities)
    }

    fn extract_relationships(&self, text: &str, entities: &[ExtractedEntity]) -> Result<Vec<ExtractedRelationship>> {
        let mut rels = Vec::new();
        let entity_names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();

        if entity_names.len() >= 2 {
            for i in 0..entity_names.len() {
                for j in (i+1)..entity_names.len() {
                    // Only create relationship if both appear near each other in text
                    let pos_i = text.to_lowercase().find(&entity_names[i].to_lowercase());
                    let pos_j = text.to_lowercase().find(&entity_names[j].to_lowercase());
                    if let (Some(pi), Some(pj)) = (pos_i, pos_j) {
                        if (pi as i64 - pj as i64).abs() < 200 {
                            rels.push(ExtractedRelationship {
                                source_name: "USER".into(),
                                target_name: entity_names[j].to_string(),
                                relation_type: "interested_in".into(),
                                fact: format!("USER mentioned {} and {}", entity_names[i], entity_names[j]),
                                confidence: 0.4,
                                scope: "global".into(),
                            });
                            break; // one relationship per entity pair
                        }
                    }
                }
            }
        }
        Ok(rels)
    }
}

/// The LLM prompt template for entity extraction.
///
/// Sent to a local model (via LM Studio or Ollama) to extract structured
/// entities and relationships from conversation text.
pub const LLM_EXTRACTION_PROMPT: &str = r#"You are an entity extraction system. Analyze the following conversation and extract:

1. **Entities**: People, technologies, projects, concepts, tools, or organizations mentioned.
2. **Relationships**: How entities relate to each other (uses, works_on, knows, interested_in, dislikes, depends_on, part_of).

Return ONLY valid JSON in this exact format:
```json
{
  "entities": [
    {
      "name": "Rust",
      "entity_type": "Technology",
      "description": "Systems programming language",
      "confidence": 0.9,
      "aliases": ["rust-lang", "Rust语言"]
    }
  ],
  "relationships": [
    {
      "source_name": "USER",
      "target_name": "Rust",
      "relation_type": "uses",
      "fact": "USER uses Rust for backend development",
      "confidence": 0.85
    }
  ]
}
```

Entity types: Person, Technology, Project, Concept, Tool, Topic, Organization
Relation types: uses, works_on, knows, interested_in, dislikes, depends_on, part_of, created_by, related_to

Conversation context:"#;

/// Parse LLM response into extracted entities and relationships.
pub fn parse_llm_extraction(response: &str) -> Result<(Vec<ExtractedEntity>, Vec<ExtractedRelationship>)> {
    // Extract JSON block from response (may be wrapped in markdown code fences)
    let json_str = if let Some(start) = response.find("```json") {
        let content = &response[start + 7..];
        if let Some(end) = content.find("```") {
            &content[..end]
        } else {
            content
        }
    } else if let Some(start) = response.find('{') {
        &response[start..]
    } else {
        return Ok((Vec::new(), Vec::new()));
    };

    let parsed: serde_json::Value = serde_json::from_str(json_str.trim())
        .map_err(|e| anyhow::anyhow!("Failed to parse LLM extraction JSON: {}", e))?;

    let entities: Vec<ExtractedEntity> = parsed["entities"]
        .as_array()
        .map(|a| a.iter().filter_map(|e| {
            Some(ExtractedEntity {
                name: e["name"].as_str()?.to_string(),
                entity_type: e["entity_type"].as_str().unwrap_or("Concept").to_string(),
                description: e["description"].as_str().unwrap_or("").to_string(),
                confidence: e["confidence"].as_f64().unwrap_or(0.5),
                aliases: e["aliases"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                scope: "global".to_string(),
            })
        }).collect())
        .unwrap_or_default();

    let relationships: Vec<ExtractedRelationship> = parsed["relationships"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| {
            Some(ExtractedRelationship {
                source_name: r["source_name"].as_str()?.to_string(),
                target_name: r["target_name"].as_str()?.to_string(),
                relation_type: r["relation_type"].as_str().unwrap_or("related_to").to_string(),
                fact: r["fact"].as_str().unwrap_or("").to_string(),
                confidence: r["confidence"].as_f64().unwrap_or(0.5),
                scope: "global".to_string(),
            })
        }).collect())
        .unwrap_or_default();

    Ok((entities, relationships))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_llm_extraction_bare_json() {
        let response = r#"{
            "entities": [{"name": "Rust", "entity_type": "Technology", "description": "PL", "confidence": 0.9, "aliases": []}],
            "relationships": [{"source_name": "USER", "target_name": "Rust", "relation_type": "uses", "fact": "USER uses Rust", "confidence": 0.85}]
        }"#;
        let (entities, relationships) = parse_llm_extraction(response).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "Rust");
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].relation_type, "uses");
    }

    #[test]
    fn test_parse_llm_extraction_code_fence() {
        let response = "Here are the results:\n```json\n{\"entities\": [], \"relationships\": []}\n```\nDone.";
        let (entities, _) = parse_llm_extraction(response).unwrap();
        assert!(entities.is_empty());
    }

    #[test]
    fn test_parse_llm_extraction_invalid() {
        let response = "No JSON here at all.";
        let (entities, relationships) = parse_llm_extraction(response).unwrap();
        assert!(entities.is_empty());
        assert!(relationships.is_empty());
    }
}
