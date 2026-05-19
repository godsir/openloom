use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct InfoRetriever;

#[async_trait::async_trait]
impl Skill for InfoRetriever {
    fn name(&self) -> &str {
        "info-retriever"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "info-retriever".into(),
            description: "Information retrieval: knowledge query and document search".into(),
            triggers: vec![
                "搜索".into(),
                "查找".into(),
                "查询".into(),
                "检索".into(),
                "信息".into(),
            ],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "query": query,
            "results": [],
            "note": "InfoRetriever: Phase 2 will integrate FTS5 + semantic search"
        }))
    }

    fn context_md(&self) -> &str {
        "Information retrieval skill: FTS5 full-text search on local documents and knowledge base."
    }
}
