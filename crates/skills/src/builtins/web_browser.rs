use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct WebBrowser;

#[async_trait::async_trait]
impl Skill for WebBrowser {
    fn name(&self) -> &str { "web-browser" }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "web-browser".into(),
            description: "Web browsing: search the web and fetch content".into(),
            triggers: vec![
                "网页".into(), "浏览".into(), "网址".into(), "链接".into(),
                "打开".into(), "搜索".into(), "百度".into(), "Google".into(),
            ],
            permissions: SkillPermissions {
                network: vec!["*".into()],
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Ok(json!({"error": "url required"}));
        }
        Ok(json!({
            "url": url,
            "status": "WebBrowser: Phase 2 will fetch and parse web content"
        }))
    }

    fn context_md(&self) -> &str {
        "Web browsing skill: open URL and fetch content. Requires network permission."
    }
}
