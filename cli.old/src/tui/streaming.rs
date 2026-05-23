use tokio::{sync::mpsc, task::JoinHandle};

pub struct StreamState {
    pub buffer: String,
    pub abort_handle: Option<JoinHandle<()>>,
    pub token_rx: Option<mpsc::Receiver<String>>,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            abort_handle: None,
            token_rx: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.token_rx.is_some()
    }

    pub fn cancel(&mut self) {
        if let Some(handle) = self.abort_handle.take() {
            handle.abort();
        }
        self.token_rx = None;
    }
}
