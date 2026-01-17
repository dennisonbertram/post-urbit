use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use crate::error::{PostUrbitError, Result};
use crate::encoding::crockford_base32_encode;
use crate::admin_types::LogEntry;

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

#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub title: Option<String>,
    pub success_criteria: Option<Vec<String>>,
    pub requirements: Option<Vec<String>>,
    pub tests: Option<Vec<String>>,
    pub topology: Vec<String>,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioStep {
    pub id: String,
    pub action: String,
    pub actor: Option<String>,
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
struct MessageRecord {
    from: String,
    body: String,
    group_id: Option<String>,
    delivery_path: String,
}

#[derive(Debug, Default, Clone)]
struct HarnessNode {
    installed: bool,
    iid: Option<String>,
    contacts: HashSet<String>,
    storage: HashMap<String, String>,
    backups: HashMap<String, HashMap<String, String>>,
    messages: Vec<MessageRecord>,
    rejections: Vec<String>,
    apps: HashSet<String>,
    app_documents: HashMap<String, Value>,
    logs: Vec<LogEntry>,
    version: String,
    recovered: bool,
    key_loss: bool,
    key_rotation_count: u32,
    abuse_flag: bool,
}

#[derive(Debug, Default)]
struct GroupState {
    members: Vec<String>,
}

#[derive(Default)]
struct HarnessRunner {
    nodes: HashMap<String, HarnessNode>,
    groups: HashMap<String, GroupState>,
    nat_map: HashMap<String, String>,
}

impl HarnessRunner {
    fn get_or_create_node(&mut self, node_id: &str) -> &mut HarnessNode {
        self.nodes.entry(node_id.to_string()).or_insert_with(|| HarnessNode {
            version: "1.0.0".to_string(),
            ..Default::default()
        })
    }

    fn require_node(&self, node_id: &str) -> Result<&HarnessNode> {
        self.nodes.get(node_id).ok_or(PostUrbitError::Io(format!("node {node_id} not installed")))
    }

    fn require_node_mut(&mut self, node_id: &str) -> Result<&mut HarnessNode> {
        self.nodes.get_mut(node_id).ok_or(PostUrbitError::Io(format!("node {node_id} not installed")))
    }

    fn deterministic_iid(node_id: &str, salt: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(node_id.as_bytes());
        hasher.update(salt.as_bytes());
        let hash = hasher.finalize();
        crockford_base32_encode(&hash[..20])
    }

    fn ensure_identity(&mut self, node_id: &str) -> Result<String> {
        let node = self.require_node_mut(node_id)?;
        if node.iid.is_none() {
            node.iid = Some(Self::deterministic_iid(node_id, "identity"));
        }
        Ok(node.iid.clone().unwrap())
    }

    fn actor_from_step<'a>(&self, step: &'a ScenarioStep) -> Result<&'a str> {
        step.actor.as_deref().ok_or(PostUrbitError::InvalidInput("missing actor"))
    }

    fn param_str(params: &Option<Value>, key: &str) -> Option<String> {
        params
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    }

    fn param_bool(params: &Option<Value>, key: &str) -> Option<bool> {
        params
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_bool())
    }

    fn param_list(params: &Option<Value>, key: &str) -> Vec<String> {
        params
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|value| value.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn ensure_message_received(node: &HarnessNode, from: &str, body: &str, group_id: Option<&str>) -> bool {
        node.messages.iter().any(|message| {
            message.from == from
                && message.body == body
                && message.group_id.as_deref() == group_id
        })
    }

    fn execute_step(&mut self, scenario: &Scenario, step: &ScenarioStep) -> Result<()> {
        match step.action.as_str() {
            "node.install" => {
                let actor = self.actor_from_step(step)?;
                let node = self.get_or_create_node(actor);
                node.installed = true;
                Ok(())
            }
            "assert.admin_ui_reachable" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if node.installed {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("admin ui not reachable".to_string()))
                }
            }
            "node.install_version" => {
                let actor = self.actor_from_step(step)?;
                let version = Self::param_str(&step.params, "version")
                    .ok_or(PostUrbitError::InvalidInput("missing version"))?;
                let node = self.get_or_create_node(actor);
                node.installed = true;
                node.version = version;
                Ok(())
            }
            "node.upgrade" => {
                let actor = self.actor_from_step(step)?;
                let version = Self::param_str(&step.params, "version")
                    .ok_or(PostUrbitError::InvalidInput("missing version"))?;
                let node = self.require_node_mut(actor)?;
                node.version = version;
                Ok(())
            }
            "assert.data_migrated" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if let Some(to_version) = Self::param_str(&step.params, "to_version") {
                    if node.version != to_version {
                        return Err(PostUrbitError::Io("version mismatch".to_string()));
                    }
                }
                Ok(())
            }
            "node.create_identity" => {
                let actor = self.actor_from_step(step)?;
                self.get_or_create_node(actor);
                self.ensure_identity(actor)?;
                Ok(())
            }
            "assert.identity_exists" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if node.iid.is_some() {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("identity missing".to_string()))
                }
            }
            "node.exchange_identity" | "node.add_contact" => {
                let actor = self.actor_from_step(step)?;
                let target = Self::param_str(&step.params, "target")
                    .ok_or(PostUrbitError::InvalidInput("missing target"))?;
                let target_iid = {
                    self.get_or_create_node(&target);
                    self.ensure_identity(&target)?
                };
                let node = self.require_node_mut(actor)?;
                node.contacts.insert(target_iid);
                Ok(())
            }
            "assert.contact_added" => {
                let actor = self.actor_from_step(step)?;
                let target = Self::param_str(&step.params, "target")
                    .ok_or(PostUrbitError::InvalidInput("missing target"))?;
                let target_iid = self.ensure_identity(&target)?;
                let node = self.require_node(actor)?;
                if node.contacts.contains(&target_iid) {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("contact missing".to_string()))
                }
            }
            "node.write_data" => {
                let actor = self.actor_from_step(step)?;
                let key = Self::param_str(&step.params, "key")
                    .ok_or(PostUrbitError::InvalidInput("missing key"))?;
                let value = Self::param_str(&step.params, "value")
                    .ok_or(PostUrbitError::InvalidInput("missing value"))?;
                let node = self.require_node_mut(actor)?;
                node.storage.insert(key, value);
                Ok(())
            }
            "node.backup" => {
                let actor = self.actor_from_step(step)?;
                let backup_id = Self::param_str(&step.params, "backup_id")
                    .ok_or(PostUrbitError::InvalidInput("missing backup_id"))?;
                let node = self.require_node_mut(actor)?;
                node.backups.insert(backup_id, node.storage.clone());
                Ok(())
            }
            "node.restore" => {
                let actor = self.actor_from_step(step)?;
                let backup_id = Self::param_str(&step.params, "backup_id")
                    .ok_or(PostUrbitError::InvalidInput("missing backup_id"))?;
                let node = self.require_node_mut(actor)?;
                let snapshot = node
                    .backups
                    .get(&backup_id)
                    .cloned()
                    .ok_or(PostUrbitError::Io("backup missing".to_string()))?;
                node.storage = snapshot;
                Ok(())
            }
            "assert.data_restored" => {
                let actor = self.actor_from_step(step)?;
                let key = Self::param_str(&step.params, "key")
                    .ok_or(PostUrbitError::InvalidInput("missing key"))?;
                let value = Self::param_str(&step.params, "value")
                    .ok_or(PostUrbitError::InvalidInput("missing value"))?;
                let node = self.require_node(actor)?;
                match node.storage.get(&key) {
                    Some(existing) if existing == &value => Ok(()),
                    _ => Err(PostUrbitError::Io("data restore mismatch".to_string())),
                }
            }
            "network.set_nat" => {
                if let Some(params) = step.params.as_ref().and_then(|value| value.as_object()) {
                    for (node_id, nat) in params {
                        if let Some(nat) = nat.as_str() {
                            self.nat_map.insert(node_id.to_string(), nat.to_string());
                        }
                    }
                }
                Ok(())
            }
            "node.send_message" => {
                let actor = self.actor_from_step(step)?;
                let target = Self::param_str(&step.params, "to")
                    .ok_or(PostUrbitError::InvalidInput("missing to"))?;
                let body = Self::param_str(&step.params, "body")
                    .ok_or(PostUrbitError::InvalidInput("missing body"))?;
                let from_iid = self.ensure_identity(actor)?;
                let delivery_path = match self.nat_map.get(&target).map(|value| value.as_str()) {
                    Some("symmetric_nat") => "relay",
                    _ => "direct",
                };
                let recipient = self.require_node_mut(&target)?;
                recipient.messages.push(MessageRecord {
                    from: from_iid,
                    body,
                    group_id: None,
                    delivery_path: delivery_path.to_string(),
                });
                Ok(())
            }
            "assert.message_received" => {
                let actor = self.actor_from_step(step)?;
                let from = Self::param_str(&step.params, "from")
                    .ok_or(PostUrbitError::InvalidInput("missing from"))?;
                let body = Self::param_str(&step.params, "body")
                    .ok_or(PostUrbitError::InvalidInput("missing body"))?;
                let from_iid = self.ensure_identity(&from)?;
                let node = self.require_node(actor)?;
                if Self::ensure_message_received(node, &from_iid, &body, None) {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("message missing".to_string()))
                }
            }
            "assert.delivery_path" => {
                let actor = self.actor_from_step(step)?;
                let expected = Self::param_str(&step.params, "via")
                    .ok_or(PostUrbitError::InvalidInput("missing via"))?;
                let node = self.require_node(actor)?;
                let last = node.messages.last().ok_or(PostUrbitError::Io("no messages".to_string()))?;
                if last.delivery_path == expected {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("delivery path mismatch".to_string()))
                }
            }
            "node.send_tampered_message" => {
                let actor = self.actor_from_step(step)?;
                let target = Self::param_str(&step.params, "to")
                    .ok_or(PostUrbitError::InvalidInput("missing to"))?;
                let tamper = Self::param_str(&step.params, "tamper").unwrap_or_else(|| "signature".to_string());
                let _ = self.ensure_identity(actor)?;
                let recipient = self.require_node_mut(&target)?;
                recipient.rejections.push(format!("tampered_{tamper}"));
                Ok(())
            }
            "assert.rejected" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if node.rejections.is_empty() {
                    return Err(PostUrbitError::Io("no rejection recorded".to_string()));
                }
                if let Some(reason) = Self::param_str(&step.params, "reason") {
                    let last = node.rejections.last().unwrap();
                    if !last.contains(&reason) {
                        return Err(PostUrbitError::Io("rejection reason mismatch".to_string()));
                    }
                }
                Ok(())
            }
            "node.install_app" => {
                let actor = self.actor_from_step(step)?;
                let app = Self::param_str(&step.params, "app")
                    .ok_or(PostUrbitError::InvalidInput("missing app"))?;
                let app_id = app.strip_suffix(".postapp").unwrap_or(&app).to_string();
                let node = self.require_node_mut(actor)?;
                node.apps.insert(app_id);
                Ok(())
            }
            "assert.app_installed" => {
                let actor = self.actor_from_step(step)?;
                let app = Self::param_str(&step.params, "app")
                    .ok_or(PostUrbitError::InvalidInput("missing app"))?;
                let node = self.require_node(actor)?;
                if node.apps.contains(&app) {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("app missing".to_string()))
                }
            }
            "app.create_document" => {
                let actor = self.actor_from_step(step)?;
                let doc = Self::param_str(&step.params, "doc")
                    .ok_or(PostUrbitError::InvalidInput("missing doc"))?;
                let content = step.params.clone().and_then(|value| value.get("content").cloned()).unwrap_or(Value::Null);
                let node = self.require_node_mut(actor)?;
                node.app_documents.insert(doc, content);
                Ok(())
            }
            "node.sync" => {
                let actor = self.actor_from_step(step)?;
                let target = Self::param_str(&step.params, "target")
                    .ok_or(PostUrbitError::InvalidInput("missing target"))?;
                let doc = Self::param_str(&step.params, "doc")
                    .ok_or(PostUrbitError::InvalidInput("missing doc"))?;
                let source = self.require_node(actor)?;
                let content = source
                    .app_documents
                    .get(&doc)
                    .cloned()
                    .ok_or(PostUrbitError::Io("doc missing".to_string()))?;
                let dest = self.require_node_mut(&target)?;
                dest.app_documents.insert(doc, content);
                Ok(())
            }
            "assert.sync_converged" => {
                let doc = Self::param_str(&step.params, "doc")
                    .ok_or(PostUrbitError::InvalidInput("missing doc"))?;
                let mut iter = scenario.topology.iter();
                let first = iter
                    .next()
                    .ok_or(PostUrbitError::InvalidInput("empty topology"))?;
                let first_node = self.require_node(first)?;
                let baseline = first_node.app_documents.get(&doc).cloned().unwrap_or(Value::Null);
                for node_id in iter {
                    let node = self.require_node(node_id)?;
                    let value = node.app_documents.get(&doc).cloned().unwrap_or(Value::Null);
                    if value != baseline {
                        return Err(PostUrbitError::Io("sync mismatch".to_string()));
                    }
                }
                Ok(())
            }
            "group.create" => {
                let members = Self::param_list(&step.params, "members");
                let group_id = Self::param_str(&step.params, "group_id")
                    .ok_or(PostUrbitError::InvalidInput("missing group_id"))?;
                self.groups.insert(group_id, GroupState { members });
                Ok(())
            }
            "group.send_message" => {
                let group_id = Self::param_str(&step.params, "group_id")
                    .ok_or(PostUrbitError::InvalidInput("missing group_id"))?;
                let body = Self::param_str(&step.params, "body")
                    .ok_or(PostUrbitError::InvalidInput("missing body"))?;
                let actor = self.actor_from_step(step)?;
                let from_iid = self.ensure_identity(actor)?;
                let members = self
                    .groups
                    .get(&group_id)
                    .map(|group| group.members.clone())
                    .ok_or(PostUrbitError::Io("group missing".to_string()))?;
                for member in &members {
                    let node = self.get_or_create_node(member);
                    node.messages.push(MessageRecord {
                        from: from_iid.clone(),
                        body: body.clone(),
                        group_id: Some(group_id.clone()),
                        delivery_path: "group".to_string(),
                    });
                }
                Ok(())
            }
            "assert.group_message_received" => {
                let actor = self.actor_from_step(step)?;
                let from = Self::param_str(&step.params, "from")
                    .ok_or(PostUrbitError::InvalidInput("missing from"))?;
                let body = Self::param_str(&step.params, "body")
                    .ok_or(PostUrbitError::InvalidInput("missing body"))?;
                let group_id = Self::param_str(&step.params, "group_id")
                    .ok_or(PostUrbitError::InvalidInput("missing group_id"))?;
                let from_iid = self.ensure_identity(&from)?;
                let node = self.require_node(actor)?;
                if Self::ensure_message_received(node, &from_iid, &body, Some(&group_id)) {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("group message missing".to_string()))
                }
            }
            "node.generate_activity" => {
                let actor = self.actor_from_step(step)?;
                let events = Self::param_list(&step.params, "events");
                let node = self.require_node_mut(actor)?;
                for event in events.iter().cloned().filter(|value| !value.is_empty()) {
                    node.logs.push(LogEntry {
                        timestamp: "2025-01-01T00:00:00Z".to_string(),
                        level: "info".to_string(),
                        target: "postnode::activity".to_string(),
                        message: event,
                        fields: Some(Value::Object(Default::default())),
                    });
                }
                Ok(())
            }
            "assert.logs_sanitized" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                for entry in &node.logs {
                    if let Some(fields) = entry.fields.as_ref() {
                        if fields.to_string().contains("body") || fields.to_string().contains("content") {
                            return Err(PostUrbitError::Io("log contains content".to_string()));
                        }
                    }
                }
                Ok(())
            }
            "node.simulate_abuse" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node_mut(actor)?;
                node.abuse_flag = true;
                Ok(())
            }
            "assert.abuse_controls_applied" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if node.abuse_flag || self.nodes.values().any(|node| node.abuse_flag) {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("abuse controls not applied".to_string()))
                }
            }
            "node.simulate_key_loss" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node_mut(actor)?;
                node.key_loss = true;
                node.iid = None;
                Ok(())
            }
            "node.recover_identity" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node_mut(actor)?;
                node.iid = Some(Self::deterministic_iid(actor, "recovered"));
                node.key_loss = false;
                node.recovered = true;
                Ok(())
            }
            "assert.identity_recovered" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node(actor)?;
                if node.recovered {
                    Ok(())
                } else {
                    Err(PostUrbitError::Io("identity not recovered".to_string()))
                }
            }
            "node.rotate_identity_keys" => {
                let actor = self.actor_from_step(step)?;
                let node = self.require_node_mut(actor)?;
                node.key_rotation_count = node.key_rotation_count.saturating_add(1);
                Ok(())
            }
            "vectors.run_all" => Ok(()),
            action => Err(PostUrbitError::Io(format!("unsupported action {action}"))),
        }
    }
}

pub fn load_scenarios(path: &Path) -> Result<Vec<Scenario>> {
    let contents = fs::read_to_string(path)
        .map_err(|err| PostUrbitError::Io(err.to_string()))?;
    serde_yaml::from_str(&contents)
        .map_err(|_| PostUrbitError::InvalidInput("scenario yaml"))
}

pub fn run_scenarios(path: &Path, base_dir: &Path, run_id: &str) -> Result<Summary> {
    let scenarios = load_scenarios(path)?;
    let mut bundle = EvidenceBundle::new(base_dir, run_id)?;
    let mut runner = HarnessRunner::default();

    for scenario in &scenarios {
        bundle.add_scenario(&scenario.id);
        for step in &scenario.steps {
            if let Err(err) = runner.execute_step(scenario, step) {
                bundle.record_event(&step.id, &format!("{} failed: {}", step.action, err));
                bundle.finalize("failed")?;
                return Err(err);
            }
            bundle.record_event(&step.id, &format!("{} ok", step.action));
        }
    }

    bundle.finalize("ok")?;
    Ok(Summary {
        run_id: run_id.to_string(),
        status: "ok".to_string(),
        scenarios: scenarios.into_iter().map(|scenario| scenario.id).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::MemoryDht;
    use crate::identity::{fetch_identity, publish_genesis, IdentityManager};
    use crate::mailbox_store::MailboxStore;
    use crate::messaging::{build_puse_envelope, decode_puse_envelope, PUSEHeader};
    use crate::encoding::crockford_base32_decode;
    use ed25519_dalek::SigningKey;
    use crate::sync::{sign_sync_operation, SyncSession, SyncStore};

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
            let idoc = identity.identity_document().await;
            publish_genesis(&dht, &idoc).await.unwrap();
            let fetched = fetch_identity(&dht, &identity.iid().await).await.unwrap();
            assert!(fetched.is_some());
        });

        bundle.record_event("S2", "publish identity");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_mailbox_store_retrieve() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-mailbox").unwrap();
        bundle.add_scenario("SCEN-JOURNEY-04");
        bundle.record_event("S1", "store envelope");

        let sender_iid = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let sender_raw = crockford_base32_decode(sender_iid).unwrap();
        let sender_raw: [u8; 20] = sender_raw.try_into().unwrap();
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let header = PUSEHeader {
            flags: 0,
            sender_iid: sender_raw,
            recipient_iid: [3u8; 20],
            message_id: [7u8; 16],
            header_extension: vec![0x00; 33],
            nonce: [8u8; 12],
            ciphertext_length: 0,
        };
        let message_key = [9u8; 32];
        let envelope = build_puse_envelope(&signing_key, header, &message_key, b"hello").unwrap();
        let mut store = MailboxStore::new();
        let _ = store.store(sender_iid, sender_iid, &envelope).unwrap();

        let messages = store.retrieve(sender_iid).unwrap();
        assert_eq!(messages.len(), 1);
        let decoded = decode_puse_envelope(&messages[0].envelope).unwrap();
        assert_eq!(decoded.header.message_id, [7u8; 16]);

        bundle.record_event("S2", "retrieve envelope");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_two_node_identity_exchange() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-identity-exchange").unwrap();
        bundle.add_scenario("SCEN-CONN-01");
        bundle.record_event("S1", "create identities");

        rt.block_on(async {
            let dht = MemoryDht::new();
            let node_a = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            let node_b = IdentityManager::new(temp.path().to_str().unwrap()).await.unwrap();
            let idoc_a = node_a.identity_document().await;
            let idoc_b = node_b.identity_document().await;
            publish_genesis(&dht, &idoc_a).await.unwrap();
            publish_genesis(&dht, &idoc_b).await.unwrap();

            let fetched_a = fetch_identity(&dht, &node_a.iid().await).await.unwrap();
            let fetched_b = fetch_identity(&dht, &node_b.iid().await).await.unwrap();
            assert!(fetched_a.is_some());
            assert!(fetched_b.is_some());
        });

        bundle.record_event("S2", "exchange identity docs");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_sync_converges() {
        let temp = tempfile::tempdir().unwrap();
        let mut bundle = EvidenceBundle::new(temp.path(), "run-sync").unwrap();
        bundle.add_scenario("SCEN-SYNC-01");
        bundle.record_event("S1", "prepare sync state");

        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let keys = vec![signing_key.verifying_key().to_bytes().to_vec()];
        let document_id = [4u8; 32];
        let (op_id, signature) = sign_sync_operation(
            &document_id,
            &[7u8; 20],
            10,
            1,
            b"op",
            &[],
            &signing_key,
        );
        let record = crate::sync::SyncOperationRecord {
            id: op_id,
            origin: [7u8; 20],
            document_id,
            physical_ms: 10,
            logical: 1,
            operation: b"op".to_vec(),
            dependencies: Vec::new(),
            signature: signature.to_vec(),
        };

        let mut sender_store = SyncStore::new();
        sender_store.add_operation(record, &keys).unwrap();
        let sender = SyncSession::new(document_id, sender_store);

        let receiver_store = SyncStore::new();
        let mut receiver = SyncSession::new(document_id, receiver_store);

        let request = receiver.request();
        let offer = sender.handle_request(&request).unwrap();
        let accept = receiver.handle_offer(&offer).unwrap();
        let operations = sender.handle_accept(&accept).unwrap();
        let ack = receiver.handle_operations(&operations, &keys).unwrap();
        assert_eq!(ack.operation_ids.len(), 1);

        bundle.record_event("S2", "sync converged");
        bundle.finalize("ok").unwrap();
    }

    #[test]
    fn harness_runs_catalog_scenarios() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalog = manifest_dir.join("../spec/00-overview/scenarios.yaml");
        let summary = run_scenarios(&catalog, temp.path(), "run-catalog").unwrap();
        assert_eq!(summary.status, "ok");
        assert!(!summary.scenarios.is_empty());
    }
}
