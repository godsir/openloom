use super::Engine;
use openloom_inference::CompletionRequest;

impl Engine {
    pub async fn stream_complete(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        if let Some(ref cloud) = self.cloud {
            cloud.complete_stream(req, tx).await
        } else {
            self.inference.complete_stream(req, tx).await
        }
    }
}
