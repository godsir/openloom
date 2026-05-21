use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct WebBrowser;

#[async_trait::async_trait]
impl Skill for WebBrowser {
    fn name(&self) -> &str {
        "web-browser"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "web-browser".into(),
            description: "Fetch web page content from a URL".into(),
            triggers: vec![
                "网页".into(),
                "浏览".into(),
                "网址".into(),
                "链接".into(),
                "打开".into(),
                "搜索".into(),
            ],
            permissions: SkillPermissions {
                network: Some(vec!["*".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Ok(
                json!({"error": "url parameter required", "usage": "{\"url\": \"https://...\"}"}),
            );
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                let text = strip_html_tags(&body);
                let truncated = if text.len() > 2000 {
                    format!("{}...[truncated]", &text[..2000])
                } else {
                    text
                };
                Ok(json!({
                    "url": url,
                    "status": status,
                    "content": truncated,
                    "length": body.len()
                }))
            }
            Err(e) => Ok(json!({
                "url": url,
                "error": e.to_string()
            })),
        }
    }

    fn context_md(&self) -> &str {
        "Web browser: fetches URL content. Pass {\"url\": \"https://...\"}. Returns text content (HTML stripped)."
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut last_was_space = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        if in_script {
            if html.contains("</script>") {
                in_script = false;
            }
            continue;
        }
        match ch {
            '\n' | '\r' | '\t' => {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            ' ' => {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            _ => {
                result.push(ch);
                last_was_space = false;
            }
        }
    }
    result.trim().to_string()
}
