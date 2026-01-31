use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::app::{DiscordUserAssociation, MacAddress};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociationEntry {
    pub association: DiscordUserAssociation,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssociationsFile {
    #[serde(default)]
    pub associations: HashMap<String, AssociationEntry>,
}

impl AssociationsFile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn to_app_state(&self) -> HashMap<MacAddress, DiscordUserAssociation> {
        self.associations
            .iter()
            .filter_map(|(mac_str, entry)| match MacAddress::parse(mac_str) {
                Some(mac) => Some((mac, entry.association)),
                None => {
                    warn!(mac = %mac_str, "Skipping invalid MAC address in associations file");
                    None
                }
            })
            .collect()
    }

    /// Merge using Last-Write-Wins strategy
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.associations.clone();

        for (mac, other_entry) in &other.associations {
            match merged.get(mac) {
                Some(our_entry) if our_entry.updated_at >= other_entry.updated_at => {
                    // Our entry is newer or same, keep it
                }
                _ => {
                    // Other entry is newer or we don't have it
                    merged.insert(mac.clone(), other_entry.clone());
                }
            }
        }

        Self {
            associations: merged,
        }
    }

    /// Update from app state, preserving timestamps for unchanged entries
    pub fn update_from_app_state(
        &self,
        state: &HashMap<MacAddress, DiscordUserAssociation>,
    ) -> Self {
        let now = Utc::now();
        let mut associations = self.associations.clone();

        for (mac, assoc) in state {
            let mac_str = mac.to_string();
            let entry = match associations.get(&mac_str) {
                Some(existing) if existing.association == *assoc => existing.clone(),
                _ => AssociationEntry {
                    association: *assoc,
                    updated_at: now,
                },
            };
            associations.insert(mac_str, entry);
        }

        Self { associations }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DiscordUserId;

    #[test]
    fn test_lww_merge() {
        let mac = "aa:bb:cc:dd:ee:ff".to_string();

        let older = AssociationsFile {
            associations: [(
                mac.clone(),
                AssociationEntry {
                    association: DiscordUserAssociation::Associated(DiscordUserId(123)),
                    updated_at: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                },
            )]
            .into_iter()
            .collect(),
        };

        let newer = AssociationsFile {
            associations: [(
                mac.clone(),
                AssociationEntry {
                    association: DiscordUserAssociation::Associated(DiscordUserId(456)),
                    updated_at: DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                },
            )]
            .into_iter()
            .collect(),
        };

        let merged = older.merge(&newer);
        let entry = merged.associations.get(&mac).unwrap();
        assert!(matches!(
            entry.association,
            DiscordUserAssociation::Associated(DiscordUserId(456))
        ));
    }

    #[test]
    fn test_mac_parsing() {
        let mac = MacAddress::parse("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(mac.to_string(), "aa:bb:cc:dd:ee:ff");
    }
}
