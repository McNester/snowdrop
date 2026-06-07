use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Shared timing envelope for query and migration-load stages.
///
/// The fields intentionally match the new architecture boundaries so reports
/// can show whether a bottleneck belongs to catalog/control-plane, execution,
/// storage, WAL, or pgwire result delivery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueryStageTimings {
    pub catalog_ms: f64,
    pub parse_ms: f64,
    pub planning_ms: f64,
    pub execution_ms: f64,
    pub wal_ms: f64,
    pub storage_ms: f64,
    pub columnar_ms: f64,
    pub pgwire_ms: f64,
}

impl QueryStageTimings {
    pub fn total_ms(&self) -> f64 {
        self.catalog_ms
            + self.parse_ms
            + self.planning_ms
            + self.execution_ms
            + self.wal_ms
            + self.storage_ms
            + self.columnar_ms
            + self.pgwire_ms
    }

    pub fn add_catalog(&mut self, duration: Duration) {
        self.catalog_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_parse(&mut self, duration: Duration) {
        self.parse_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_planning(&mut self, duration: Duration) {
        self.planning_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_execution(&mut self, duration: Duration) {
        self.execution_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_wal(&mut self, duration: Duration) {
        self.wal_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_storage(&mut self, duration: Duration) {
        self.storage_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_columnar(&mut self, duration: Duration) {
        self.columnar_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn add_pgwire(&mut self, duration: Duration) {
        self.pgwire_ms += duration.as_secs_f64() * 1000.0;
    }
}

#[derive(Debug)]
pub struct QueryStageTimer {
    started_at: Instant,
}

impl QueryStageTimer {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_stage_timings() {
        let mut timings = QueryStageTimings::default();
        timings.add_catalog(Duration::from_millis(1));
        timings.add_parse(Duration::from_millis(2));
        timings.add_execution(Duration::from_millis(3));
        timings.add_storage(Duration::from_millis(4));

        assert_eq!(timings.total_ms(), 10.0);
    }
}
