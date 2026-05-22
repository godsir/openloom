use serde::{Deserialize, Serialize};

/// Stub replacement for the real OTel-based SessionTelemetry.
///
/// All methods are no-ops; the `counter` and other recording functions accept
/// the same arguments as the real implementation so that callers compile
/// without modification.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionTelemetry;

impl SessionTelemetry {
    pub fn counter(&self, _name: &str, _inc: u64, _attrs: &[(&str, &str)]) {
        // no-op stub
    }

    pub fn gauge(&self, _name: &str, _val: f64, _attrs: &[(&str, &str)]) {
        // no-op stub
    }

    pub fn histogram(&self, _name: &str, _val: f64, _attrs: &[(&str, &str)]) {
        // no-op stub
    }
}

/// Stub for runtime metrics summary (future use).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeMetricsSummary;

/// Authentication mode for telemetry export.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TelemetryAuthMode {
    #[default]
    Disabled,
}

/// Accumulated runtime metric totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeMetricTotals {
    pub count: u64,
    pub duration_ms: u64,
}

impl RuntimeMetricTotals {
    pub fn is_empty(self) -> bool {
        self.count == 0
    }

    pub fn merge(&mut self, other: Self) {
        self.count += other.count;
        self.duration_ms += other.duration_ms;
    }
}
