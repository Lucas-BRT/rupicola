//! Ephemeral Git worktrees for building and capturing each tree in a job.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use rupicola_api::TreeSpec;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Strategy for preparing a buildable checkout of a [`TreeSpec`] (e.g. git worktree, container, Nix).
#[async_trait]
pub trait TreeCheckout: Send + Sync {
    async fn checkout(
        &self,
        repo_root: &Path,
        scratch_dir: &Path,
        tree: &TreeSpec,
    ) -> Result<Worktree>;
}

/// Default [`TreeCheckout`] backed by detached `git worktree add` + `git merge`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitWorktreeCheckout;

#[async_trait]
impl TreeCheckout for GitWorktreeCheckout {
    async fn checkout(
        &self,
        repo_root: &Path,
        scratch_dir: &Path,
        tree: &TreeSpec,
    ) -> Result<Worktree> {
        Worktree::create(repo_root, scratch_dir, tree).await
    }
}

const GIT_USER_NAME: &str = "rupicola-runner";
const GIT_USER_EMAIL: &str = "runner@rupicola.local";

/// A checked-out Git worktree in a scratch directory, with its merged revision.
pub struct Worktree {
    repo_root: PathBuf,
    path: PathBuf,
    revision: String,
    removed: bool,
}

impl Worktree {
    /// Creates a detached worktree for `tree` under `scratch_dir` and merges `tree.merge` in order.
    ///
    /// Cleans up the worktree if any merge fails.
    ///
    /// # Errors
    ///
    /// Fails when `git worktree add`, `git merge`, or `git rev-parse` fails.
    pub async fn create(repo_root: &Path, scratch_dir: &Path, tree: &TreeSpec) -> Result<Self> {
        let path = scratch_dir.join(&tree.name);
        let add = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "add", "--detach"])
            .arg(&path)
            .arg(&tree.base)
            .output()
            .await
            .context("run git worktree add")?;
        if !add.status.success() {
            let stderr = String::from_utf8_lossy(&add.stderr);
            bail!(
                "git worktree add for `{}` ({}) failed: {}",
                tree.name,
                tree.base,
                stderr.trim()
            );
        }

        let mut worktree = Self {
            repo_root: repo_root.to_owned(),
            path,
            revision: String::new(),
            removed: false,
        };

        for merge_ref in &tree.merge {
            let merge = Command::new("git")
                .arg("-C")
                .arg(&worktree.path)
                .args([
                    "-c",
                    &format!("user.name={GIT_USER_NAME}"),
                    "-c",
                    &format!("user.email={GIT_USER_EMAIL}"),
                    "merge",
                    "--no-edit",
                    merge_ref,
                ])
                .output()
                .await
                .context("run git merge")?;
            if !merge.status.success() {
                let stderr = String::from_utf8_lossy(&merge.stderr);
                let stdout = String::from_utf8_lossy(&merge.stdout);
                let _ = worktree.remove().await;
                bail!(
                    "git merge `{merge_ref}` into `{}` failed: {} {}",
                    tree.name,
                    stdout.trim(),
                    stderr.trim()
                );
            }
        }

        let rev = Command::new("git")
            .arg("-C")
            .arg(&worktree.path)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .await
            .context("run git rev-parse")?;
        if !rev.status.success() {
            let _ = worktree.remove().await;
            bail!("git rev-parse failed for `{}`", tree.name);
        }
        worktree.revision = String::from_utf8_lossy(&rev.stdout).trim().to_owned();
        Ok(worktree)
    }

    /// Path of the checked-out worktree on disk.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Short commit hash of the worktree's HEAD after all merges.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Removes the worktree from Git and deletes its directory.
    ///
    /// # Errors
    ///
    /// Fails only if the directory still exists and cannot be removed.
    pub async fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        self.removed = true;
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .output()
            .await;
        if output.as_ref().is_ok_and(|out| out.status.success()) {
            return Ok(());
        }
        if self.path.exists() {
            tokio::fs::remove_dir_all(&self.path)
                .await
                .with_context(|| format!("remove worktree {}", self.path.display()))?;
        }
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["worktree", "prune"])
            .output()
            .await;
        Ok(())
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        if !self.removed && self.path.exists() {
            let _ = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.repo_root)
                .args(["worktree", "remove", "--force"])
                .arg(&self.path)
                .output();
            if self.path.exists() {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn init_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "develop"]);
        git(&repo, &["config", "user.name", "Test"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        std::fs::write(repo.join("file.txt"), "base\n").unwrap();
        git(&repo, &["add", "file.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        (dir, repo)
    }

    #[tokio::test]
    async fn worktree_merges_cleanly_records_revision_and_cleans_up() {
        let (dir, repo) = init_repo();
        git(&repo, &["checkout", "-b", "feat/ok"]);
        std::fs::write(repo.join("other.txt"), "feat\n").unwrap();
        git(&repo, &["add", "other.txt"]);
        git(&repo, &["commit", "-m", "feat"]);
        git(&repo, &["checkout", "develop"]);

        let scratch = dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let tree = TreeSpec {
            name: "fixed".to_owned(),
            base: "develop".to_owned(),
            merge: vec!["feat/ok".to_owned()],
            fixes: Vec::new(),
        };
        let mut wt = Worktree::create(&repo, &scratch, &tree).await.unwrap();
        assert_ne!(wt.revision(), "");
        assert!(wt.path().join("other.txt").exists());
        let wt_path = wt.path().to_owned();
        wt.remove().await.unwrap();
        assert!(!wt_path.exists());
    }

    #[tokio::test]
    async fn worktree_conflict_fails_and_cleans_up() {
        let (dir, repo) = init_repo();
        git(&repo, &["checkout", "-b", "feat/conflict"]);
        std::fs::write(repo.join("file.txt"), "branch\n").unwrap();
        git(&repo, &["commit", "-am", "branch"]);
        git(&repo, &["checkout", "develop"]);
        std::fs::write(repo.join("file.txt"), "develop\n").unwrap();
        git(&repo, &["commit", "-am", "develop"]);

        let scratch = dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let tree = TreeSpec {
            name: "broken".to_owned(),
            base: "develop".to_owned(),
            merge: vec!["feat/conflict".to_owned()],
            fixes: Vec::new(),
        };
        let err = Worktree::create(&repo, &scratch, &tree)
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("merge"), "{err}");
        assert!(!scratch.join("broken").exists());
    }

    #[test]
    fn git_worktree_checkout_implements_tree_checkout_trait() {
        fn assert_impl<T: TreeCheckout>() {}
        assert_impl::<GitWorktreeCheckout>();
    }
}
