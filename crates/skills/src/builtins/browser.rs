use anyhow::Result;
use serde_json::{Value, json};

use crate::{Skill, SkillManifest, SkillPermissions};

pub struct BrowserSkill;

#[async_trait::async_trait]
impl Skill for BrowserSkill {
    fn name(&self) -> &str {
        "browser"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "browser".into(),
            description: "Browse the web: fetch pages, extract content, and search. Supports navigate (fetch URL), snapshot (extract text), and search (DuckDuckGo HTML search).".into(),
            triggers: vec![
                "浏览".into(),
                "搜索".into(),
                "查一下".into(),
                "上网".into(),
                "fetch".into(),
                "browse".into(),
            ],
            permissions: SkillPermissions {
                network: Some(vec!["*".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("navigate");

        match action {
            "navigate" => self.navigate(&params).await,
            "snapshot" => self.snapshot(&params).await,
            "search" => self.search(&params).await,
            _ => Ok(json!({
                "error": "unknown action",
                "available": ["navigate", "snapshot", "search"]
            })),
        }
    }

    fn context_md(&self) -> &str {
        "browser: browse the web — navigate to URLs, get text snapshots, search DuckDuckGo."
    }
}

impl BrowserSkill {
    async fn navigate(&self, params: &Value) -> Result<Value> {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");

        if url.is_empty() {
            return Ok(json!({"error": "'url' is required"}));
        }

        let url = if !url.starts_with("http") {
            format!("https://{}", url)
        } else {
            url.to_string()
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("openLoom-browser/0.2")
            .build()?;

        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string();
                let body = resp.text().await.unwrap_or_default();
                let title = extract_title(&body);
                let text = strip_html(&body);
                let truncated = if text.len() > 4000 {
                    format!(
                        "{}...\n\n[Content truncated: {} total chars]",
                        &text[..4000],
                        text.len()
                    )
                } else {
                    text
                };
                Ok(json!({
                    "url": url,
                    "status": status,
                    "content_type": content_type,
                    "title": title,
                    "text": truncated,
                }))
            }
            Err(e) => Ok(json!({
                "url": url,
                "error": e.to_string(),
                "hint": if e.is_timeout() { "Request timed out. The server may be slow or unreachable." } else if e.is_connect() { "Could not connect to the server. Check the URL." } else { "Request failed." }
            })),
        }
    }

    async fn snapshot(&self, params: &Value) -> Result<Value> {
        // Snapshot is the same as navigate but returns only text
        self.navigate(params).await
    }

    async fn search(&self, params: &Value) -> Result<Value> {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");

        if query.is_empty() {
            return Ok(json!({"error": "'query' is required for web search"}));
        }

        // Use DuckDuckGo HTML search (no API key needed)
        let search_url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("openLoom-browser/0.2")
            .build()?;

        match client.get(&search_url).send().await {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                let results = extract_search_results(&body);
                Ok(json!({
                    "query": query,
                    "results": results,
                    "results_count": results.len(),
                }))
            }
            Err(e) => Ok(json!({
                "query": query,
                "error": e.to_string(),
            })),
        }
    }
}

fn extract_title(html: &str) -> String {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<title")
        && let Some(gt) = lower[start..].find('>')
    {
        let content_start = start + gt + 1;
        if let Some(end) = lower[content_start..].find("</title>") {
            return html_decode(&html[content_start..content_start + end]);
        }
    }
    String::new()
}

fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_style = false;
    let mut in_script = false;
    let mut last_was_newline = false;

    let lower = html.to_lowercase();

    // Ridiculous but zero-dependency: track position manually
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Check for closing tags
            if i + 7 < bytes.len() && lower.as_bytes()[i..].starts_with(b"<script") {
                in_script = true;
                in_tag = true;
            } else if i + 6 < bytes.len() && lower.as_bytes()[i..].starts_with(b"<style") {
                in_style = true;
                in_tag = true;
            } else if in_script
                && i + 8 < bytes.len()
                && lower.as_bytes()[i..].starts_with(b"</script>")
            {
                in_script = false;
                in_tag = false;
                i += 9;
                continue;
            } else if in_style
                && i + 7 < bytes.len()
                && lower.as_bytes()[i..].starts_with(b"</style>")
            {
                in_style = false;
                in_tag = false;
                i += 8;
                continue;
            } else if !in_script && !in_style {
                in_tag = true;
            }
            if !in_script && !in_style {
                i += 1;
                continue;
            }
        }
        if in_tag {
            if bytes[i] == b'>' {
                in_tag = false;
                // Block-level elements add newline
                // simple heuristic: just add space
            }
            i += 1;
            continue;
        }
        if in_script || in_style {
            i += 1;
            continue;
        }
        // Whitespace normalization
        if bytes[i] == b'\n' || bytes[i] == b'\r' || bytes[i] == b'\t' {
            if !last_was_newline {
                result.push('\n');
                last_was_newline = true;
            }
        } else if bytes[i] == b' ' {
            if !last_was_newline {
                result.push(' ');
                last_was_newline = true;
            }
        } else {
            result.push(html.as_bytes()[i] as char);
            last_was_newline = false;
        }
        i += 1;
    }

    // Decode common HTML entities
    html_decode(&result)
}

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' => {
                c.to_string()
            }
            _ => {
                let bytes = c.to_string().into_bytes();
                bytes
                    .iter()
                    .map(|b| format!("%{:02X}", b))
                    .collect::<Vec<_>>()
                    .join("")
            }
        })
        .collect()
}

fn extract_search_results(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let mut remaining = html;

    for _ in 0..10 {
        let pos = match remaining.find("result__a") {
            Some(p) => p,
            None => break,
        };
        // Find href
        let hs = match remaining[pos..].find("href=\"") {
            Some(h) => h,
            None => {
                remaining = &remaining[(pos + 50).min(remaining.len())..];
                continue;
            }
        };
        let href_pos = pos + hs + 6;
        let href_end = remaining[href_pos..].find('"').unwrap_or(0);
        if href_end == 0 {
            remaining = &remaining[(pos + 50).min(remaining.len())..];
            continue;
        }
        let url = html_decode(&remaining[href_pos..href_pos + href_end]);

        let gt = remaining[href_pos + href_end..].find('>').unwrap_or(0);
        let text_start = href_pos + href_end + gt + 1;
        let text_end = remaining[text_start..].find("</a>").unwrap_or(0);
        if text_end == 0 {
            remaining = &remaining[(pos + 50).min(remaining.len())..];
            continue;
        }
        let title = strip_html(&remaining[text_start..text_start + text_end]);

        if !url.is_empty() && !title.is_empty() {
            let snippet = remaining[text_start + text_end..]
                .find("result__snippet")
                .and_then(|sp| {
                    let sp_start = text_start + text_end + sp;
                    let gt2 = remaining[sp_start..].find('>')?;
                    let end = remaining[sp_start + gt2 + 1..].find("</").unwrap_or(0);
                    Some(strip_html(
                        &remaining[sp_start + gt2 + 1..sp_start + gt2 + 1 + end],
                    ))
                })
                .unwrap_or_default();
            results.push(json!({
                "title": title.trim(),
                "url": url.trim(),
                "snippet": snippet.trim(),
            }));
        }
        remaining = &remaining[(pos + 200).min(remaining.len())..];
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>Hello World</title></head></html>";
        assert_eq!(extract_title(html), "Hello World");
    }

    #[test]
    fn test_strip_html() {
        let html = "<p>Hello <b>World</b></p>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_html_decode() {
        assert_eq!(html_decode("a&amp;b"), "a&b");
        assert_eq!(html_decode("a&lt;b"), "a<b");
        assert_eq!(html_decode("a&gt;b"), "a>b");
    }
}
