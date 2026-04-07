use anyhow::Result;
use git2::{DiffDelta, DiffOptions, Repository};
use std::path::Path;

pub use super::{ChangeType, CommitInfo, FileChange};

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
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

        let mut opts = DiffOptions::new();
        let diff =
            self.repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;

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

        // Intern file paths to avoid repeated String allocations in O(n²) loop
        let mut path_intern: std::collections::HashMap<&str, u32> =
            std::collections::HashMap::new();
        let mut path_list: Vec<&str> = Vec::new();
        for commit in &commits {
            for f in &commit.files_changed {
                let next_id = path_list.len() as u32;
                if let std::collections::hash_map::Entry::Vacant(e) =
                    path_intern.entry(f.path.as_str())
                {
                    e.insert(next_id);
                    path_list.push(f.path.as_str());
                }
            }
        }

        // Count coupling using interned IDs (u32 pairs, not String pairs)
        let mut coupling: std::collections::HashMap<(u32, u32), usize> =
            std::collections::HashMap::new();
        for commit in &commits {
            let file_ids: Vec<u32> = commit
                .files_changed
                .iter()
                .filter_map(|f| path_intern.get(f.path.as_str()).copied())
                .collect();

            for i in 0..file_ids.len() {
                for j in (i + 1)..file_ids.len() {
                    let pair = if file_ids[i] < file_ids[j] {
                        (file_ids[i], file_ids[j])
                    } else {
                        (file_ids[j], file_ids[i])
                    };
                    *coupling.entry(pair).or_insert(0) += 1;
                }
            }
        }

        let mut pairs: Vec<_> = coupling.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        pairs.truncate(limit);

        Ok(pairs
            .into_iter()
            .map(|((a, b), count)| {
                (
                    path_list[a as usize].to_string(),
                    path_list[b as usize].to_string(),
                    count,
                )
            })
            .collect())
    }

    /// Get file churn (most frequently changed files)
    pub fn file_churn(&self, limit: usize) -> Result<Vec<(String, usize)>> {
        let commits = self.recent_commits(500)?;
        let mut churn: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        for commit in &commits {
            for file in &commit.files_changed {
                *churn.entry(file.path.as_str()).or_insert(0) += 1;
            }
        }

        // Only allocate Strings for the top results
        let mut files: Vec<_> = churn.into_iter().collect();
        files.sort_by(|a, b| b.1.cmp(&a.1));
        files.truncate(limit);

        Ok(files.into_iter().map(|(p, c)| (p.to_string(), c)).collect())
    }

    /// Get contributor map (who knows what)
    pub fn contributor_map(
        &self,
    ) -> Result<std::collections::HashMap<String, Vec<(String, usize)>>> {
        let commits = self.recent_commits(1000)?;
        let mut map: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
            std::collections::HashMap::with_capacity(50);

        for commit in &commits {
            // Only clone author/path on first insertion (entry API handles this)
            let author_map = map.entry(commit.author.clone()).or_default();
            for file in &commit.files_changed {
                *author_map.entry(file.path.clone()).or_insert(0) += 1;
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
