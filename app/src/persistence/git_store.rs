use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use atomic_write_file::AtomicWriteFile;
use git2::{Cred, FetchOptions, Oid, PushOptions, RemoteCallbacks, Repository, Signature};
use std::io::Write;
use tracing::{debug, error, info, warn};

use super::association::AssociationsFile;
use crate::app::{DiscordUserAssociation, HasPersistingAssociationState, MacAddress};

pub struct GitStore {
    repo_path: PathBuf,
    file_name: String,
    remote_name: String,
    branch_name: String,
    ssh_key_path: Option<PathBuf>,
}

impl GitStore {
    pub fn new(
        repo_path: PathBuf,
        file_name: String,
        remote_name: String,
        branch_name: String,
        ssh_key_path: Option<PathBuf>,
    ) -> Self {
        Self {
            repo_path,
            file_name,
            remote_name,
            branch_name,
            ssh_key_path,
        }
    }

    fn file_path(&self) -> PathBuf {
        self.repo_path.join(&self.file_name)
    }

    fn create_callbacks(&self) -> RemoteCallbacks<'_> {
        let mut callbacks = RemoteCallbacks::new();
        let ssh_key_path = self.ssh_key_path.clone();

        callbacks.credentials(move |_url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                if let Some(ref key_path) = ssh_key_path {
                    Cred::ssh_key(username, None, key_path, None)
                } else {
                    Cred::ssh_key_from_agent(username)
                }
            } else if allowed_types.contains(git2::CredentialType::DEFAULT) {
                Cred::default()
            } else {
                Err(git2::Error::from_str("no valid credential types available"))
            }
        });

        callbacks
    }

    pub fn validate_repo(&self) -> Result<()> {
        let repo = Repository::open(&self.repo_path).context("Failed to open git repository")?;
        repo.find_remote(&self.remote_name).with_context(|| {
            format!(
                "Git remote '{}' not found in repository",
                self.remote_name
            )
        })?;
        let local_ref = format!("refs/heads/{}", self.branch_name);
        let remote_ref = format!("refs/remotes/{}/{}", self.remote_name, self.branch_name);
        if repo.find_reference(&local_ref).is_err() && repo.find_reference(&remote_ref).is_err() {
            anyhow::bail!(
                "Git branch '{}' not found locally or as {}",
                self.branch_name,
                remote_ref
            );
        }
        Ok(())
    }

    fn fetch_and_merge(&self, repo: &Repository) -> Result<Option<AssociationsFile>> {
        let mut remote = repo.find_remote(&self.remote_name)?;

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(self.create_callbacks());

        let refspec = format!("refs/heads/{}", self.branch_name);
        if let Err(e) = remote.fetch(&[&refspec], Some(&mut fetch_opts), None) {
            warn!("Failed to fetch from remote: {}", e);
            return Ok(None);
        }

        let fetch_head = repo.find_reference("FETCH_HEAD")?;
        let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;

        let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

        if analysis.is_up_to_date() {
            debug!("Already up to date with remote");
            return Ok(None);
        }

        if analysis.is_fast_forward() {
            let mut reference = repo.find_reference(&format!("refs/heads/{}", self.branch_name))?;
            reference.set_target(fetch_commit.id(), "Fast-forward")?;
            repo.set_head(&format!("refs/heads/{}", self.branch_name))?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            info!("Fast-forwarded to remote");
        }

        if !analysis.is_fast_forward() {
            warn!("Remote branch diverged from local; merging associations using fetched commit");
            return self.read_file_from_commit(repo, fetch_commit.id());
        }

        // Read remote file if it exists (fast-forwarded)
        self.read_file_from_worktree()
    }

    fn read_file_from_worktree(&self) -> Result<Option<AssociationsFile>> {
        let file_path = self.file_path();
        if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)?;
            return Ok(Some(AssociationsFile::from_json(&content)?));
        }

        Ok(None)
    }

    fn read_file_from_commit(
        &self,
        repo: &Repository,
        commit_id: Oid,
    ) -> Result<Option<AssociationsFile>> {
        let commit = repo.find_commit(commit_id)?;
        let tree = commit.tree()?;
        let entry = match tree.get_path(Path::new(&self.file_name)) {
            Ok(entry) => entry,
            Err(_) => return Ok(None),
        };
        let blob = repo.find_blob(entry.id())?;
        let content =
            std::str::from_utf8(blob.content()).context("Associations file is not valid UTF-8")?;
        Ok(Some(AssociationsFile::from_json(content)?))
    }

    fn commit_and_push(&self, repo: &Repository, content: &str) -> Result<()> {
        // Write file atomically
        let file_path = self.file_path();
        let mut atomic_file = AtomicWriteFile::open(&file_path)?;
        atomic_file.write_all(content.as_bytes())?;
        atomic_file.commit()?;

        // Stage the file
        let mut index = repo.index()?;
        index.add_path(std::path::Path::new(&self.file_name))?;
        index.write()?;

        // Create commit
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let signature = Signature::now("NetworkMonitorBot", "bot@localhost")?;

        let head = repo.head()?;
        let parent = repo.find_commit(head.target().context("HEAD has no target")?)?;

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Update associations",
            &tree,
            &[&parent],
        )?;

        info!("Created commit for association update");

        // Push to remote
        let mut remote = repo.find_remote(&self.remote_name)?;
        let mut push_opts = PushOptions::new();
        push_opts.remote_callbacks(self.create_callbacks());

        let refspec = format!(
            "refs/heads/{}:refs/heads/{}",
            self.branch_name, self.branch_name
        );
        if let Err(e) = remote.push(&[&refspec], Some(&mut push_opts)) {
            error!("Failed to push to remote: {}", e);
            // Don't fail the whole operation, local state is still valid
        } else {
            info!("Pushed to remote");
        }

        Ok(())
    }

    fn load_local(&self) -> Result<AssociationsFile> {
        let file_path = self.file_path();
        if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)?;
            AssociationsFile::from_json(&content)
        } else {
            Ok(AssociationsFile::new())
        }
    }
}

impl HasPersistingAssociationState for GitStore {
    async fn reconcile_and_persist_association_state(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> HashMap<MacAddress, DiscordUserAssociation> {
        let repo_path = self.repo_path.clone();
        let file_name = self.file_name.clone();
        let remote_name = self.remote_name.clone();
        let branch_name = self.branch_name.clone();
        let ssh_key_path = self.ssh_key_path.clone();

        let fallback = current.clone().unwrap_or_default();

        let result = tokio::task::spawn_blocking(move || {
            let store = GitStore::new(repo_path, file_name, remote_name, branch_name, ssh_key_path);
            store.reconcile_sync(current)
        })
        .await;

        match result {
            Ok(Ok(state)) => state,
            Ok(Err(e)) => {
                error!("Failed to reconcile state: {}", e);
                error!("Persistence failed; in-memory associations may diverge until next retry");
                fallback
            }
            Err(e) => {
                error!("Blocking task panicked: {}", e);
                error!("Persistence failed; in-memory associations may diverge until next retry");
                fallback
            }
        }
    }
}

impl GitStore {
    fn reconcile_sync(
        &self,
        current: Option<HashMap<MacAddress, DiscordUserAssociation>>,
    ) -> Result<HashMap<MacAddress, DiscordUserAssociation>> {
        let repo = Repository::open(&self.repo_path).context("Failed to open git repository")?;

        // Load local file
        let local = self.load_local()?;

        // Fetch and get remote state
        let remote = self.fetch_and_merge(&repo)?;

        // Merge with remote if available
        let merged = match remote {
            Some(remote_file) => local.merge(&remote_file),
            None => local,
        };

        // If we have current app state, update with it
        let final_state = match current {
            Some(app_state) => {
                let updated = merged.update_from_app_state(&app_state);

                // Only commit if there are changes
                let merged_json = merged.to_json()?;
                let updated_json = updated.to_json()?;

                if merged_json != updated_json {
                    self.commit_and_push(&repo, &updated_json)?;
                }

                updated.to_app_state()
            }
            None => {
                // Initial load, just return merged state
                info!(
                    "GitStore::reconcile_sync: no current application state; returning merged state without committing"
                );
                merged.to_app_state()
            }
        };

        Ok(final_state)
    }
}
