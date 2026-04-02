use anyhow::Result;
use git2::{Repository, DiffDelta, DiffOptions};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub timestamp: i64,
    pub message: String,
    pub files_changed: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: ChangeType,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Analyzes git history for temporal code evolution
pub struct GitAnalyzer {
    repo: Repository,
}

impl GitAnalyzer {
    pub fn open(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)?;
        Ok(Self { repo })
    }

    /// Get the last N commits
    pub fn recent_commits(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut commits = Vec::new();

        for oid in revwalk.take(limit) {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;

            let author = commit.author();
            let author_name = author.name().unwrap_or("unknown").to_string();
            let message = commit.message().unwrap_or("").to_string();

            // Get diff for this commit
            let files_changed = self.commit_diff(&commit)?;

            commits.push(CommitInfo {
                hash: oid.to_string(),
                author: author_name,
                timestamp: commit.time().seconds(),
                message,
                files_changed,
            });
        }

        Ok(commits)
    }

    /// Get file changes for a specific commit
    fn commit_diff(&self, commit: &git2::Commit) -> Result<Vec<FileChange>> {
        let tree = commit.tree()?;
        let parent_tree = commit
            .parent(0)
            .ok()
            .and_then(|p| p.tree().ok());

        let mut opts = DiffOptions::new();
        let diff = self.repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut opts),
        )?;

        let mut changes = Vec::new();

        diff.foreach(
            &mut |delta: DiffDelta, _progress| {
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let change_type = match delta.status() {
                    git2::Delta::Added => ChangeType::Added,
                    git2::Delta::Deleted => ChangeType::Deleted,
                    git2::Delta::Renamed => ChangeType::Renamed,
                    _ => ChangeType::Modified,
                };

                changes.push(FileChange {
                    path,
                    change_type,
                    additions: 0,
                    deletions: 0,
                });

                true
            },
            None,
            None,
            None,
        )?;

        Ok(changes)
    }

    /// Get files most frequently changed together (change coupling)
    pub fn change_coupling(&self, limit: usize) -> Result<Vec<(String, String, usize)>> {
        let commits = self.recent_commits(500)?;
        let mut coupling: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();

        for commit in &commits {
            let files: Vec<&str> = commit
                .files_changed
                .iter()
                .map(|f| f.path.as_str())
                .collect();

            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let pair = if files[i] < files[j] {
                        (files[i].to_string(), files[j].to_string())
                    } else {
                        (files[j].to_string(), files[i].to_string())
                    };
                    *coupling.entry(pair).or_insert(0) += 1;
                }
            }
        }

        let mut pairs: Vec<_> = coupling.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        pairs.truncate(limit);

        Ok(pairs.into_iter().map(|((a, b), count)| (a, b, count)).collect())
    }

    /// Get file churn (most frequently changed files)
    pub fn file_churn(&self, limit: usize) -> Result<Vec<(String, usize)>> {
        let commits = self.recent_commits(500)?;
        let mut churn: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for commit in &commits {
            for file in &commit.files_changed {
                *churn.entry(file.path.clone()).or_insert(0) += 1;
            }
        }

        let mut files: Vec<_> = churn.into_iter().collect();
        files.sort_by(|a, b| b.1.cmp(&a.1));
        files.truncate(limit);

        Ok(files)
    }

    /// Get contributor map (who knows what)
    pub fn contributor_map(&self) -> Result<std::collections::HashMap<String, Vec<(String, usize)>>> {
        let commits = self.recent_commits(1000)?;
        let mut map: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
            std::collections::HashMap::new();

        for commit in &commits {
            let author = &commit.author;
            for file in &commit.files_changed {
                *map.entry(author.clone())
                    .or_default()
                    .entry(file.path.clone())
                    .or_insert(0) += 1;
            }
        }

        // Convert to sorted vecs
        Ok(map
            .into_iter()
            .map(|(author, files)| {
                let mut files: Vec<_> = files.into_iter().collect();
                files.sort_by(|a, b| b.1.cmp(&a.1));
                (author, files)
            })
            .collect())
    }
}
