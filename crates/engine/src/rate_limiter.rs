use anyhow::Result;
use std::time::Instant;

pub(crate) struct RateLimiter {
    pub(crate) last_request: Instant,
    pub(crate) min_interval_ms: u64,
}

impl RateLimiter {
    pub(crate) fn new(min_interval_ms: u64) -> Self {
        Self {
            last_request: Instant::now() - std::time::Duration::from_millis(min_interval_ms),
            min_interval_ms,
        }
    }

    pub(crate) fn check(&mut self) -> Result<()> {
        let elapsed = self.last_request.elapsed();
        let min_dur = std::time::Duration::from_millis(self.min_interval_ms);
        if elapsed < min_dur {
            anyhow::bail!(
                "rate limit exceeded, retry in {}ms",
                (min_dur - elapsed).as_millis()
            );
        }
        self.last_request = Instant::now();
        Ok(())
    }
}
