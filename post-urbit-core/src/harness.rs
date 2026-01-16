use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{PostUrbitError, Result};

#[derive(Debug, Serialize)]
pub struct Summary {
    pub run_id: String,
    pub status: String,
    pub scenarios: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Event {
    pub step_id: String,
    pub detail: String,
}

pub struct EvidenceBundle {
    base_dir: PathBuf,
    run_id: String,
    events: Vec<Event>,
    scenarios: Vec<String>,
}

impl EvidenceBundle {
    pub fn new(base_dir: &Path, run_id: &str) -> Result<Self> {
        let run_dir = base_dir.join(run_id);
        fs::create_dir_all(run_dir.join("artifacts"))
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        Ok(Self {
            base_dir: run_dir,
            run_id: run_id.to_string(),
            events: Vec::new(),
            scenarios: Vec::new(),
        })
    }

    pub fn add_scenario(&mut self, scenario: &str) {
        self.scenarios.push(scenario.to_string());
    }

    pub fn record_event(&mut self, step_id: &str, detail: &str) {
        self.events.push(Event {
            step_id: step_id.to_string(),
            detail: detail.to_string(),
        });
    }

    pub fn finalize(self, status: &str) -> Result<()> {
        let summary = Summary {
            run_id: self.run_id.clone(),
            status: status.to_string(),
            scenarios: self.scenarios,
        };
        let summary_json = serde_json::to_vec_pretty(&summary)
            .map_err(|_| PostUrbitError::InvalidInput("summary json"))?;
        let summary_md = format!(
            "# Run {run_id}\n\nStatus: {status}\n",
            run_id = summary.run_id,
            status = summary.status
        );

        let summary_path = self.base_dir.join("summary.md");
        fs::write(summary_path, summary_md)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        let summary_json_path = self.base_dir.join("summary.json");
        fs::write(summary_json_path, summary_json)
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;

        let mut events_file = File::create(self.base_dir.join("events.ndjson"))
            .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        for event in self.events {
            let line = serde_json::to_string(&event)
                .map_err(|_| PostUrbitError::InvalidInput("event json"))?;
            writeln!(events_file, "{line}")
                .map_err(|err| PostUrbitError::Io(err.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::MemoryDht;
    use crate::identity::{fetch_identity, publish_genesis, IdentityManager};

    #[test]
    fn evidence_bundle_writes_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-test").unwrap();
        bundle.add_scenario("SCEN-ID-01");
        bundle.record_event("S1", "created identity");
        bundle.finalize("ok").unwrap();

        assert!(temp.path().join("run-test/summary.md").exists());
        assert!(temp.path().join("run-test/summary.json").exists());
        assert!(temp.path().join("run-test/events.ndjson").exists());
    }

    #[test]
    fn harness_single_node_identity_publish() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-e2e").unwrap();
        bundle.add_scenario("SCEN-ID-01");
        bundle.record_event("S1", "create identity");

        rt.block_on(async {
            let dht = MemoryDht::new();
            let identity = IdentityManager::new(temp.path().to_str().unwrap())
                .await
                .unwrap();
            publish_genesis(&dht, identity.identity_document()).await.unwrap();
            let fetched = fetch_identity(&dht, identity.iid()).await.unwrap();
            assert!(fetched.is_some());
        });

        bundle.record_event("S2", "publish identity");
        bundle.finalize("ok").unwrap();
    }
}
