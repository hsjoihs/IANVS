use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use tokio::fs;
use tracing::{error, info};

use crate::app::{DiscordUserAssociation, HasPersistingAssociationState, MacAddress};

#[derive(Debug, Clone)]
pub struct JsonFileAssociationPersistence {
    path: PathBuf,
}

impl JsonFileAssociationPersistence {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    async fn load_persisted(&self) -> HashMap<MacAddress, DiscordUserAssociation> {
        let persisted_contents = match fs::read(&self.path).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return HashMap::new();
            }
            Err(err) => {
                error!(
                    "Failed to read associations file {}: {err}",
                    self.path.display()
                );
                return HashMap::new();
            }
        };

        match serde_json::from_slice(&persisted_contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                error!(
                    "Failed to parse associations file {}: {err}",
                    self.path.display()
                );
                return HashMap::new();
            }
        }
    }

    async fn persist(
        &self,
        map: &HashMap<MacAddress, DiscordUserAssociation>,
    ) -> anyhow::Result<()> {
        if let Some(parent) = Path::new(&self.path).parent() {
            if let Err(err) = fs::create_dir_all(parent).await {
                anyhow::bail!(
                    "Failed to create associations directory {}: {err}",
                    parent.display()
                );
            }
        }

        let json = match serde_json::to_vec_pretty(&map) {
            Ok(json) => json,
            Err(err) => {
                anyhow::bail!(
                    "Failed to serialize associations for {}: {err}",
                    self.path.display()
                );
            }
        };

        if let Err(err) = fs::write(&self.path, json).await {
            anyhow::bail!(
                "Failed to write associations file {}: {err}",
                self.path.display()
            );
        }

        Ok(())
    }
}

impl HasPersistingAssociationState for JsonFileAssociationPersistence {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation> {
        let persisted = self.load_persisted().await;
        let mut result = persisted.clone();

        let merged = match current {
            Some(current) => {
                for (mac, association) in current {
                    if let Some(existing) = persisted.get(&mac) {
                        if existing != &association {
                            info!(
                                "Overriding persisted association for {} (persisted: {:?}, current: {:?})",
                                mac, existing, association
                            );
                        }
                    }
                    result.insert(mac, association);
                }
                result
            }
            None => persisted,
        };

        if let Err(err) = self.persist(&merged).await {
            error!("Failed to persist merged association state: {err}");
        }

        merged
    }
}
