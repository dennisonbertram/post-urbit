use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::encoding::{crockford_base32_encode, validate_crockford_base32_lower};
use crate::error::{PostUrbitError, Result};
use crate::ratchet::kdf_sender_key;

#[derive(Debug, Clone)]
pub struct SenderKey {
    pub key_id: [u8; 16],
    pub sender_iid: [u8; 20],
    pub chain_key: [u8; 32],
    pub created_at: String,
    pub iteration: u32,
}

impl SenderKey {
    pub fn advance(&mut self, group_id: &[u8; 20]) -> Result<[u8; 32]> {
        let (new_chain, message_key) =
            kdf_sender_key(&self.chain_key, group_id, &self.sender_iid, &self.key_id);
        self.chain_key = new_chain;
        self.iteration = self
            .iteration
            .checked_add(1)
            .ok_or(PostUrbitError::InvalidInput("iteration overflow"))?;
        Ok(message_key)
    }
}

pub fn generate_sender_key(sender_iid: [u8; 20], created_at: &str) -> Result<SenderKey> {
    validate_timestamp(created_at)?;
    let mut key_id = [0u8; 16];
    let mut chain_key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key_id);
    rand::rngs::OsRng.fill_bytes(&mut chain_key);
    Ok(SenderKey {
        key_id,
        sender_iid,
        chain_key,
        created_at: created_at.to_string(),
        iteration: 0,
    })
}

pub fn should_rotate_sender_key(key: &SenderKey, now: DateTime<Utc>) -> Result<bool> {
    let created_at = key
        .created_at
        .parse::<DateTime<Utc>>()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    let too_many_messages = key.iteration >= 100;
    let too_old = now.signed_duration_since(created_at).num_days() >= 7;
    Ok(too_many_messages || too_old)
}

pub fn derive_group_id(
    creator_iid_raw: &[u8; 20],
    random: &[u8; 32],
    created_at: &str,
) -> Result<String> {
    validate_timestamp(created_at)?;
    let mut hasher = Sha256::new();
    hasher.update(creator_iid_raw);
    hasher.update(random);
    hasher.update(created_at.as_bytes());
    let digest = hasher.finalize();
    Ok(crockford_base32_encode(&digest[..20]).to_lowercase())
}

fn validate_timestamp(value: &str) -> Result<()> {
    if value.contains('.') {
        return Err(PostUrbitError::InvalidInput("timestamp fractional"));
    }
    if value.len() != 20 || !value.ends_with('Z') {
        return Err(PostUrbitError::InvalidInput("timestamp format"));
    }
    let _: DateTime<Utc> = value
        .parse()
        .map_err(|_| PostUrbitError::InvalidInput("timestamp parse"))?;
    Ok(())
}

pub fn validate_group_id(group_id: &str) -> Result<()> {
    validate_crockford_base32_lower(group_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupRole {
    Owner,
    Admin,
    Moderator,
    Member,
}

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub iid: String,
    pub role: GroupRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupAction {
    AddMember,
    RemoveMember,
    PromoteAdmin,
    DemoteAdmin,
    UpdateInfo,
    RotateSenderKey,
}

impl GroupAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupAction::AddMember => "add_member",
            GroupAction::RemoveMember => "remove_member",
            GroupAction::PromoteAdmin => "promote_admin",
            GroupAction::DemoteAdmin => "demote_admin",
            GroupAction::UpdateInfo => "update_info",
            GroupAction::RotateSenderKey => "rotate_sender_key",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GroupStateUpdate {
    pub action: GroupAction,
    pub group_id: String,
    pub target_iid: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct GroupStateUpdateInternal {
    pub action: GroupAction,
    pub group_id: String,
    pub target_iid: Option<String>,
    pub version: String,
    pub actor_iid: String,
}

#[derive(Debug, Clone)]
pub struct GroupState {
    pub group_id: String,
    pub version: String,
    pub members: std::collections::HashMap<String, GroupMember>,
    last_update: Option<GroupStateUpdateInternal>,
}

impl GroupState {
    pub fn new(group_id: &str, creator_iid: &str) -> Result<Self> {
        validate_group_id(group_id)?;
        validate_crockford_base32_lower(creator_iid)?;
        let version = format!("0.{}", &creator_iid[..8]);
        let mut members = std::collections::HashMap::new();
        members.insert(
            creator_iid.to_string(),
            GroupMember {
                iid: creator_iid.to_string(),
                role: GroupRole::Owner,
            },
        );
        Ok(Self {
            group_id: group_id.to_string(),
            version,
            members,
            last_update: None,
        })
    }

    pub fn apply_update(&mut self, update: GroupStateUpdateInternal) -> Result<()> {
        if update.group_id != self.group_id {
            return Err(PostUrbitError::InvalidInput("group id mismatch"));
        }
        validate_group_version(&update.version, &update.actor_iid)?;
        if let Some(current) = &self.last_update {
            if compare_updates(&update, current) != std::cmp::Ordering::Greater {
                return Ok(());
            }
        }

        let actor = self
            .members
            .get(&update.actor_iid)
            .ok_or(PostUrbitError::InvalidInput("actor not member"))?;
        let target_role = update
            .target_iid
            .as_ref()
            .and_then(|iid| self.members.get(iid))
            .map(|member| member.role.clone());

        match update.action {
            GroupAction::AddMember => match actor.role {
                GroupRole::Owner | GroupRole::Admin | GroupRole::Moderator => {}
                _ => return Err(PostUrbitError::InvalidInput("role not allowed")),
            },
            GroupAction::RemoveMember => {
                let Some(target) = update.target_iid.as_ref() else {
                    return Err(PostUrbitError::InvalidInput("missing target"));
                };
                if target == &update.actor_iid {
                    // allow self-removal
                } else {
                    match actor.role {
                        GroupRole::Owner | GroupRole::Admin => {}
                        GroupRole::Moderator => {
                            if target_role != Some(GroupRole::Member) {
                                return Err(PostUrbitError::InvalidInput("role not allowed"));
                            }
                        }
                        _ => return Err(PostUrbitError::InvalidInput("role not allowed")),
                    }
                }
            }
            GroupAction::PromoteAdmin | GroupAction::DemoteAdmin => match actor.role {
                GroupRole::Owner => {}
                _ => return Err(PostUrbitError::InvalidInput("role not allowed")),
            },
            GroupAction::UpdateInfo | GroupAction::RotateSenderKey => match actor.role {
                GroupRole::Owner | GroupRole::Admin => {}
                _ => return Err(PostUrbitError::InvalidInput("role not allowed")),
            },
        }

        match update.action {
            GroupAction::AddMember => {
                let Some(target) = update.target_iid.clone() else {
                    return Err(PostUrbitError::InvalidInput("missing target"));
                };
                validate_crockford_base32_lower(&target)?;
                self.members.entry(target.clone()).or_insert(GroupMember {
                    iid: target,
                    role: GroupRole::Member,
                });
            }
            GroupAction::RemoveMember => {
                let Some(target) = update.target_iid.clone() else {
                    return Err(PostUrbitError::InvalidInput("missing target"));
                };
                self.members.remove(&target);
            }
            GroupAction::PromoteAdmin => {
                let Some(target) = update.target_iid.clone() else {
                    return Err(PostUrbitError::InvalidInput("missing target"));
                };
                if let Some(member) = self.members.get_mut(&target) {
                    member.role = GroupRole::Admin;
                }
            }
            GroupAction::DemoteAdmin => {
                let Some(target) = update.target_iid.clone() else {
                    return Err(PostUrbitError::InvalidInput("missing target"));
                };
                if let Some(member) = self.members.get_mut(&target) {
                    member.role = GroupRole::Member;
                }
            }
            GroupAction::UpdateInfo | GroupAction::RotateSenderKey => {}
        }

        self.version = update.version.clone();
        self.last_update = Some(update);
        Ok(())
    }
}

pub fn validate_group_version(version: &str, actor_iid: &str) -> Result<()> {
    let (clock, suffix) = parse_group_version(version)?;
    if clock == 0 && !version.starts_with("0.") {
        return Err(PostUrbitError::InvalidInput("version format"));
    }
    if !actor_iid.starts_with(&suffix) {
        return Err(PostUrbitError::InvalidInput("version actor mismatch"));
    }
    Ok(())
}

fn parse_group_version(version: &str) -> Result<(u64, String)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 2 {
        return Err(PostUrbitError::InvalidInput("version format"));
    }
    let clock = parts[0]
        .parse::<u64>()
        .map_err(|_| PostUrbitError::InvalidInput("version clock"))?;
    let suffix = parts[1].to_string();
    if suffix.len() != 8 {
        return Err(PostUrbitError::InvalidInput("version suffix"));
    }
    Ok((clock, suffix))
}

fn compare_updates(a: &GroupStateUpdateInternal, b: &GroupStateUpdateInternal) -> std::cmp::Ordering {
    let (a_clock, a_suffix) = parse_group_version(&a.version).unwrap_or((0, String::new()));
    let (b_clock, b_suffix) = parse_group_version(&b.version).unwrap_or((0, String::new()));
    let order = a_clock.cmp(&b_clock);
    if order != std::cmp::Ordering::Equal {
        return order;
    }
    let order = a_suffix.cmp(&b_suffix);
    if order != std::cmp::Ordering::Equal {
        return order;
    }
    let order = a.actor_iid.cmp(&b.actor_iid);
    if order != std::cmp::Ordering::Equal {
        return order;
    }
    a.action.as_str().cmp(b.action.as_str())
}

 

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_id_derivation_is_deterministic() {
        let creator = [1u8; 20];
        let random = [2u8; 32];
        let created_at = "2025-01-13T12:00:00Z";
        let id1 = derive_group_id(&creator, &random, created_at).unwrap();
        let id2 = derive_group_id(&creator, &random, created_at).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 32);
    }

    #[test]
    fn sender_key_iteration_increments() {
        let mut key = SenderKey {
            key_id: [7u8; 16],
            sender_iid: [8u8; 20],
            chain_key: [9u8; 32],
            created_at: "2025-01-13T12:00:00Z".to_string(),
            iteration: 0,
        };
        let group_id = [1u8; 20];
        let _ = key.advance(&group_id).unwrap();
        assert_eq!(key.iteration, 1);
    }

    #[test]
    fn sender_key_rotation_triggers() {
        let key = SenderKey {
            key_id: [7u8; 16],
            sender_iid: [8u8; 20],
            chain_key: [9u8; 32],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            iteration: 100,
        };
        let now = "2025-01-10T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(should_rotate_sender_key(&key, now).unwrap());
    }

    #[test]
    fn sender_key_generation_validates_timestamp() {
        let key = generate_sender_key([1u8; 20], "2025-01-13T12:00:00Z").unwrap();
        assert_eq!(key.iteration, 0);
    }

    #[test]
    fn group_version_validation_enforces_suffix() {
        let actor = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        validate_group_version("1.b1n7cfsc", actor).unwrap();
        let err = validate_group_version("1.deadbeef", actor).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn group_state_rejects_unauthorized_promotion() {
        let group_id = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let creator = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let mut state = GroupState::new(group_id, creator).unwrap();
        state.members.insert(
            "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
            GroupMember {
                iid: "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
                role: GroupRole::Member,
            },
        );
        let update = GroupStateUpdateInternal {
            action: GroupAction::PromoteAdmin,
            group_id: group_id.to_string(),
            target_iid: Some("42kbzq2tyab939amybd76bm8kfpzgn95".to_string()),
            version: "1.b1n7cfsc".to_string(),
            actor_iid: "42kbzq2tyab939amybd76bm8kfpzgn95".to_string(),
        };
        let err = state.apply_update(update).unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }

    #[test]
    fn group_state_applies_add_member() {
        let group_id = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let creator = "b1n7cfscgashm32xx7eaxw0y09gy0y2v";
        let mut state = GroupState::new(group_id, creator).unwrap();
        let update = GroupStateUpdateInternal {
            action: GroupAction::AddMember,
            group_id: group_id.to_string(),
            target_iid: Some("42kbzq2tyab939amybd76bm8kfpzgn95".to_string()),
            version: "1.b1n7cfsc".to_string(),
            actor_iid: creator.to_string(),
        };
        state.apply_update(update).unwrap();
        assert!(state.members.contains_key("42kbzq2tyab939amybd76bm8kfpzgn95"));
    }
}
