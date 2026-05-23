use anyhow::{Context, anyhow};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// List available files in a ModelScope repo.
pub async fn list_files(repo: &str) -> anyhow::Result<Vec<FileEntry>> {
    let url = format!(
        "https://www.modelscope.cn/api/v1/models/{repo}/repo/files?Revision=master&Recursive=True"
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "openLoom/0.1.0")
        .send()
        .await
        .context("Failed to fetch file list from ModelScope")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "ModelScope returned {} for repo '{}'. Check the repo ID.",
            resp.status(),
            repo
        );
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse file list JSON")?;
    let files = body["Data"]["Files"]
        .as_array()
        .ok_or_else(|| anyhow!("Unexpected API response: missing Data.Files array"))?;

    let mut entries: Vec<FileEntry> = files
        .iter()
        .filter_map(|f| {
            Some(FileEntry {
                name: f["Name"].as_str()?.to_string(),
                size: f["Size"].as_u64()?,
            })
        })
        .collect();

    if entries.is_empty() {
        anyhow::bail!("No files found in repo '{}'", repo);
    }

    entries.sort_by_key(|e| e.size);
    Ok(entries)
}

/// Download a single file from ModelScope.
pub async fn download_file(repo: &str, file: &str, dest: &Path, force: bool) -> anyhow::Result<()> {
    if dest.exists() && !force {
        tracing::info!(
            "File already exists at '{}' — skipping. Use --force to re-download.",
            dest.display()
        );
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    let url = format!(
        "https://www.modelscope.cn/api/v1/models/{repo}/repo?Revision=master&FilePath={file}"
    );

    tracing::info!("Downloading {} from {}...", file, repo);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "openLoom/0.1.0")
        .send()
        .await
        .context("Failed to start download")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "ModelScope returned {} for file '{}' in repo '{}'",
            resp.status(),
            file,
            repo
        );
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut last_report: u64 = 0;
    let report_interval: u64 = 10 * 1024 * 1024; // 10 MiB

    let tmp_path = dest.with_extension("tmp");
    let mut out = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("Failed to create temp file '{}'", tmp_path.display()))?;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download stream error")?;
        out.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if downloaded - last_report >= report_interval {
            if total > 0 {
                let pct = (downloaded as f64 / total as f64) * 100.0;
                tracing::info!(
                    "  {:.1}% ({:.1} / {:.1} MiB)",
                    pct,
                    downloaded as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0
                );
            } else {
                tracing::info!("  {:.1} MiB downloaded", downloaded as f64 / 1_048_576.0);
            }
            last_report = downloaded;
        }
    }

    out.flush().await?;
    drop(out);

    tokio::fs::rename(&tmp_path, dest).await.with_context(|| {
        format!(
            "Failed to rename '{}' to '{}'",
            tmp_path.display(),
            dest.display()
        )
    })?;

    tracing::info!(
        "Downloaded '{}' ({:.1} MiB) -> '{}'",
        file,
        downloaded as f64 / 1_048_576.0,
        dest.display()
    );

    Ok(())
}

pub struct FileEntry {
    pub name: String,
    pub size: u64,
}

/// Resolve the output directory for model downloads.
pub fn default_output_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openLoom")
        .join("models")
}

pub struct DownloadOpts {
    pub repo: String,
    pub file: String,
    pub output: Option<PathBuf>,
    pub list: bool,
    pub force: bool,
}

pub async fn run(opts: DownloadOpts) -> anyhow::Result<()> {
    if opts.list {
        let files = list_files(&opts.repo).await?;
        println!("Available files in {}:", opts.repo);
        for f in &files {
            let size_str = if f.size >= 1_073_741_824 {
                format!("{:.2} GB", f.size as f64 / 1_073_741_824.0)
            } else {
                format!("{:.1} MB", f.size as f64 / 1_048_576.0)
            };
            println!("  {:40}  {}", f.name, size_str);
        }
        return Ok(());
    }

    let dir = opts.output.unwrap_or_else(default_output_dir);
    let dest = dir.join(&opts.file);

    download_file(&opts.repo, &opts.file, &dest, opts.force).await
}
