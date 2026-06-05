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
    fn extract_entities(&self, text: &str, context: &str, scope: &str) -> Result<Vec<ExtractedEntity>>;
    fn extract_relationships(
        &self,
        text: &str,
        entities: &[ExtractedEntity],
        scope: &str,
    ) -> Result<Vec<ExtractedRelationship>>;
}

/// Rule-based extractor using keyword matching and simple patterns.
pub struct RuleBasedEntityExtractor;

impl EntityExtractor for RuleBasedEntityExtractor {
    fn extract_entities(&self, text: &str, _context: &str, scope: &str) -> Result<Vec<ExtractedEntity>> {
        let mut entities = Vec::new();
        let lower = text.to_lowercase();

        // Technology detection via common keywords
        let techs: &[(&str, &str, &str)] = &[
            // ── Programming Languages ──────────────────────────────
            ("rust", "Technology", "Systems programming language"),
            ("python", "Technology", "General-purpose programming language"),
            ("typescript", "Technology", "Typed JavaScript superset"),
            ("golang", "Technology", "Go programming language"),
            ("java", "Technology", "JVM-based object-oriented language"),
            ("javascript", "Technology", "Dynamic scripting language for the web"),
            ("c++", "Technology", "General-purpose systems language"),
            ("c#", "Technology", ".NET object-oriented language"),
            ("kotlin", "Technology", "Modern JVM language for Android/backend"),
            ("swift", "Technology", "Apple's systems programming language"),
            ("elixir", "Technology", "Functional language on Erlang VM"),
            ("scala", "Technology", "JVM language blending OOP and FP"),
            ("zig", "Technology", "Modern C alternative with safety features"),
            ("nix", "Technology", "Purely functional package manager/language"),
            ("lua", "Technology", "Lightweight embeddable scripting language"),
            ("ruby", "Technology", "Dynamic scripting language (Rails)"),
            ("php", "Technology", "Server-side web scripting language"),
            ("perl", "Technology", "Versatile scripting language"),
            ("haskell", "Technology", "Purely functional programming language"),
            // ── Frameworks & Libraries ─────────────────────────────
            ("react", "Technology", "UI framework by Meta"),
            ("vue", "Technology", "Progressive JavaScript framework"),
            ("angular", "Technology", "Web application framework by Google"),
            ("svelte", "Technology", "Compile-time reactive UI framework"),
            ("next.js", "Technology", "React meta-framework for SSR/SSG"),
            ("nuxt", "Technology", "Vue meta-framework"),
            ("tauri", "Technology", "Rust-based desktop app framework"),
            ("electron", "Technology", "Chromium-based desktop app framework"),
            ("fastapi", "Technology", "Modern Python web framework"),
            ("django", "Technology", "Batteries-included Python web framework"),
            ("flask", "Technology", "Lightweight Python web micro-framework"),
            ("spring", "Technology", "Java enterprise application framework"),
            ("torch", "Technology", "PyTorch ML framework"),
            ("tensorflow", "Technology", "Google's ML framework"),
            ("transformers", "Technology", "HuggingFace transformer models library"),
            ("langchain", "Technology", "LLM application framework"),
            ("axum", "Technology", "Rust async web framework"),
            ("tokio", "Technology", "Rust async runtime"),
            ("bevy", "Technology", "Rust game/ECS engine"),
            // ── Databases ──────────────────────────────────────────
            ("postgres", "Technology", "Advanced open-source relational DB"),
            ("postgresql", "Technology", "Advanced open-source relational DB"),
            ("sqlite", "Technology", "Embedded zero-config database"),
            ("mysql", "Technology", "Popular open-source relational database"),
            ("redis", "Technology", "In-memory data structure store"),
            ("mongodb", "Technology", "Document-oriented NoSQL database"),
            ("elasticsearch", "Technology", "Distributed search and analytics engine"),
            ("neo4j", "Technology", "Graph database"),
            ("clickhouse", "Technology", "Columnar OLAP database"),
            ("duckdb", "Technology", "In-process analytical database"),
            ("cassandra", "Technology", "Wide-column distributed NoSQL database"),
            ("milvus", "Technology", "Vector database for embedding search"),
            ("qdrant", "Technology", "Rust-based vector search engine"),
            ("chroma", "Technology", "Open-source vector database for AI"),
            // ── DevOps & Infrastructure ────────────────────────────
            ("docker", "Tool", "Container platform"),
            ("kubernetes", "Tool", "Container orchestration platform"),
            ("k8s", "Tool", "Kubernetes (container orchestration)"),
            ("terraform", "Tool", "Infrastructure-as-code by HashiCorp"),
            ("ansible", "Tool", "IT automation and configuration management"),
            ("jenkins", "Tool", "CI/CD automation server"),
            ("github actions", "Tool", "GitHub CI/CD pipeline"),
            ("gitlab ci", "Tool", "GitLab CI/CD pipeline"),
            ("nginx", "Tool", "High-performance reverse-proxy/web server"),
            ("grafana", "Tool", "Observability dashboards and analytics"),
            ("prometheus", "Tool", "Monitoring and alerting toolkit"),
            ("kafka", "Tool", "Distributed event streaming platform"),
            ("rabbitmq", "Tool", "Message broker implementing AMQP"),
            ("nats", "Tool", "Cloud-native messaging system"),
            ("consul", "Tool", "Service mesh and service discovery"),
            // ── AI / ML ────────────────────────────────────────────
            ("ai", "Concept", "Artificial Intelligence"),
            ("machine learning", "Concept", "Machine Learning"),
            ("deep learning", "Concept", "Deep Learning"),
            ("llm", "Concept", "Large Language Model"),
            ("rag", "Concept", "Retrieval-Augmented Generation"),
            ("embedding", "Concept", "Vector representation of data"),
            ("transformer", "Concept", "Attention-based neural architecture"),
            ("diffusion", "Concept", "Generative image model family"),
            ("fine-tuning", "Concept", "Model fine-tuning"),
            ("ft", "Concept", "Fine-tuning"),
            ("prompt engineering", "Concept", "Crafting effective LLM prompts"),
            ("claude", "Tool", "Anthropic's AI assistant"),
            ("openai", "Organization", "AI research organization (GPT)"),
            ("deepseek", "Organization", "Chinese AI model provider"),
            ("qwen", "Organization", "Alibaba's Tongyi Qianwen LLM family"),
            ("glm", "Organization", "Tsinghua's GLM / ChatGLM model family"),
            ("ollama", "Tool", "Local LLM runner"),
            ("lm studio", "Tool", "Desktop local LLM runner"),
            ("agent", "Concept", "Autonomous AI agent system"),
            ("mcp", "Concept", "Model Context Protocol"),
            ("lsp", "Concept", "Language Server Protocol"),
            ("skill", "Concept", "Modular agent capability / plugin"),
            ("langchain", "Technology", "LLM application framework"),
            ("copilot", "Tool", "GitHub Copilot AI coding assistant"),
            ("chatgpt", "Tool", "OpenAI's conversational AI product"),
            // ── Tools & Editors ────────────────────────────────────
            ("git", "Tool", "Distributed version control system"),
            ("github", "Tool", "Code hosting and collaboration platform"),
            ("gitlab", "Tool", "DevOps and code hosting platform"),
            ("vscode", "Tool", "Visual Studio Code editor"),
            ("neovim", "Tool", "Modern Vim-based text editor"),
            ("intellij", "Tool", "JetBrains Java IDE"),
            // ── Concepts & Architecture ────────────────────────────
            ("microservices", "Concept", "Distributed service architecture"),
            ("graphql", "Concept", "API query language"),
            ("grpc", "Concept", "High-performance RPC framework"),
            ("rest", "Concept", "Representational State Transfer"),
            ("webassembly", "Concept", "Wasm — portable binary instruction format"),
            ("wasm", "Concept", "WebAssembly runtime"),
            ("serverless", "Concept", "Event-driven compute without server management"),
            ("crdt", "Concept", "Conflict-free Replicated Data Type"),
            ("edge computing", "Concept", "Compute at network edge"),
            // ── Organizations & Projects ───────────────────────────
            ("openloom", "Project", "Personal AI kernel / assistant"),
            ("雷火", "Organization", "Leihuo / NetEase gaming studio"),
        ];
        for (kw, etype, desc) in techs {
            if lower.contains(kw) {
                entities.push(ExtractedEntity {
                    name: kw.to_string(),
                    entity_type: etype.to_string(),
                    description: desc.to_string(),
                    confidence: 0.6,
                    aliases: vec![],
                    scope: scope.into(),
                });
            }
        }
        Ok(entities)
    }

    fn extract_relationships(
        &self,
        text: &str,
        entities: &[ExtractedEntity],
        scope: &str,
    ) -> Result<Vec<ExtractedRelationship>> {
        let mut rels = Vec::new();
        let entity_names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();

        if entity_names.len() >= 2 {
            for i in 0..entity_names.len() {
                for j in (i + 1)..entity_names.len() {
                    // Only create relationship if both appear near each other in text
                    let pos_i = text.to_lowercase().find(&entity_names[i].to_lowercase());
                    let pos_j = text.to_lowercase().find(&entity_names[j].to_lowercase());
                    if let (Some(pi), Some(pj)) = (pos_i, pos_j)
                        && (pi as i64 - pj as i64).abs() < 200
                    {
                        rels.push(ExtractedRelationship {
                            source_name: "USER".into(),
                            target_name: entity_names[j].to_string(),
                            relation_type: "interested_in".into(),
                            fact: format!(
                                "USER mentioned {} and {}",
                                entity_names[i], entity_names[j]
                            ),
                            confidence: 0.4,
                            scope: scope.into(),
                        });
                        break; // one relationship per entity pair
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
pub const LLM_EXTRACTION_PROMPT: &str = r#"You are an entity extraction system. Analyze the conversation and extract entities and their relationships.

**Entity types**: Person, Technology, Project, Concept, Tool, Topic, Organization
**Relation types**: uses, works_on, knows, interested_in, dislikes, depends_on, part_of, created_by, related_to

Return ONLY valid JSON with "entities" and "relationships" arrays.
Confidence: 0.0–1.0 (high when explicit, lower when implied).

**Rules**:
- Extract both USER↔entity and entity↔entity relationships (e.g., "FastAPI is a Python framework" → Python-[part_of]→FastAPI).
- Detect sentiment in Chinese: "喜欢"→interested_in, "不喜欢/讨厌"→dislikes.
- When "做/开发/写/部署" connects two entities, create a relationship.

**Examples**:

Input: "我用 React 做前端，后端用 Python 的 FastAPI，用 Docker 部署"
{
  "entities": [
    {"name": "React", "entity_type": "Technology", "description": "前端 UI 框架", "confidence": 0.9, "aliases": []},
    {"name": "Python", "entity_type": "Technology", "description": "通用编程语言", "confidence": 0.9, "aliases": []},
    {"name": "FastAPI", "entity_type": "Technology", "description": "Python Web 框架", "confidence": 0.85, "aliases": []},
    {"name": "Docker", "entity_type": "Tool", "description": "容器化平台", "confidence": 0.85, "aliases": []},
    {"name": "前端", "entity_type": "Concept", "description": "用户界面开发", "confidence": 0.7, "aliases": ["frontend"]}
  ],
  "relationships": [
    {"source_name": "USER", "target_name": "React", "relation_type": "uses", "fact": "USER uses React for frontend", "confidence": 0.9},
    {"source_name": "USER", "target_name": "Python", "relation_type": "uses", "fact": "USER uses Python with FastAPI for backend", "confidence": 0.9},
    {"source_name": "Python", "target_name": "FastAPI", "relation_type": "part_of", "fact": "FastAPI is a Python framework", "confidence": 0.85},
    {"source_name": "USER", "target_name": "Docker", "relation_type": "uses", "fact": "USER uses Docker for deployment", "confidence": 0.85}
  ]
}

Input: "我们项目用微服务和前后端分离架构。我用 Rust 写了一个 MCP server，部署在 K8s 上。"
{
  "entities": [
    {"name": "微服务", "entity_type": "Concept", "description": "微服务架构模式", "confidence": 0.85, "aliases": ["microservices"]},
    {"name": "前后端分离", "entity_type": "Concept", "description": "前后端分离架构", "confidence": 0.8, "aliases": []},
    {"name": "Rust", "entity_type": "Technology", "description": "Systems programming language", "confidence": 0.9, "aliases": []},
    {"name": "MCP", "entity_type": "Concept", "description": "Model Context Protocol", "confidence": 0.85, "aliases": ["MCP server"]},
    {"name": "K8s", "entity_type": "Technology", "description": "Kubernetes", "confidence": 0.85, "aliases": ["Kubernetes"]}
  ],
  "relationships": [
    {"source_name": "USER", "target_name": "微服务", "relation_type": "uses", "fact": "USER's project uses microservices", "confidence": 0.85},
    {"source_name": "USER", "target_name": "前后端分离", "relation_type": "uses", "fact": "USER's project uses frontend-backend separation", "confidence": 0.8},
    {"source_name": "USER", "target_name": "Rust", "relation_type": "uses", "fact": "USER wrote an MCP server in Rust", "confidence": 0.9},
    {"source_name": "MCP", "target_name": "K8s", "relation_type": "depends_on", "fact": "MCP server is deployed on Kubernetes", "confidence": 0.8}
  ]
}

Input: "我不太喜欢 JavaScript，太动态了。还是 TypeScript 舒服。后端用 Rust。"
{
  "entities": [
    {"name": "JavaScript", "entity_type": "Technology", "description": "Dynamic scripting language", "confidence": 0.9, "aliases": ["JS"]},
    {"name": "TypeScript", "entity_type": "Technology", "description": "Typed JavaScript superset", "confidence": 0.9, "aliases": ["TS"]},
    {"name": "Rust", "entity_type": "Technology", "description": "Systems programming language", "confidence": 0.9, "aliases": []}
  ],
  "relationships": [
    {"source_name": "USER", "target_name": "JavaScript", "relation_type": "dislikes", "fact": "USER dislikes JavaScript because it is too dynamic", "confidence": 0.85},
    {"source_name": "USER", "target_name": "TypeScript", "relation_type": "interested_in", "fact": "USER prefers TypeScript for its type safety", "confidence": 0.85},
    {"source_name": "USER", "target_name": "Rust", "relation_type": "uses", "fact": "USER uses Rust for backend", "confidence": 0.9}
  ]
}

Conversation context:"#;

/// Parse LLM response into extracted entities and relationships.
pub fn parse_llm_extraction(
    response: &str,
    scope: &str,
) -> Result<(Vec<ExtractedEntity>, Vec<ExtractedRelationship>)> {
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
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some(ExtractedEntity {
                        name: e["name"].as_str()?.to_string(),
                        entity_type: e["entity_type"].as_str().unwrap_or("Concept").to_string(),
                        description: e["description"].as_str().unwrap_or("").to_string(),
                        confidence: e["confidence"].as_f64().unwrap_or(0.5),
                        aliases: e["aliases"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        scope: scope.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let relationships: Vec<ExtractedRelationship> = parsed["relationships"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    Some(ExtractedRelationship {
                        source_name: r["source_name"].as_str()?.to_string(),
                        target_name: r["target_name"].as_str()?.to_string(),
                        relation_type: r["relation_type"]
                            .as_str()
                            .unwrap_or("related_to")
                            .to_string(),
                        fact: r["fact"].as_str().unwrap_or("").to_string(),
                        confidence: r["confidence"].as_f64().unwrap_or(0.5),
                        scope: scope.to_string(),
                    })
                })
                .collect()
        })
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
        let (entities, relationships) = parse_llm_extraction(response, "global").unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "Rust");
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].relation_type, "uses");
    }

    #[test]
    fn test_parse_llm_extraction_code_fence() {
        let response =
            "Here are the results:\n```json\n{\"entities\": [], \"relationships\": []}\n```\nDone.";
        let (entities, _) = parse_llm_extraction(response, "global").unwrap();
        assert!(entities.is_empty());
    }

    #[test]
    fn test_parse_llm_extraction_invalid() {
        let response = "No JSON here at all.";
        let (entities, relationships) = parse_llm_extraction(response, "global").unwrap();
        assert!(entities.is_empty());
        assert!(relationships.is_empty());
    }
}
