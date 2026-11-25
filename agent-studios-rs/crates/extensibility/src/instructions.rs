//! Hierarchical Repository Instructions Resolver (`AGENTS.md` / `CLAUDE.md`)
//!
//! Enforces strict worktree boundary containment: instructions cannot escape
//! the assigned `worktree_root` via `..` traversal or symlinks.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionFileKind {
    AgentsMd,
    ClaudeMd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionScope {
    Global,
    WorktreeRoot,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionEntry {
    pub kind: InstructionFileKind,
    pub scope: InstructionScope,
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionResolution {
    pub worktree_root: PathBuf,
    pub active_cwd: PathBuf,
    pub entries: Vec<InstructionEntry>,
}

impl InstructionResolution {
    pub fn merged_prompt_context(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&format!(
                "<!-- Instruction Source ({:?}): {} -->\n{}",
                entry.scope,
                entry.path.display(),
                entry.content.trim()
            ));
        }
        out
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InstructionError {
    #[error("Worktree boundary violation: {0}")]
    BoundaryViolation(String),
    #[error("Instruction I/O error: {0}")]
    Io(String),
}

pub struct InstructionResolver;

impl InstructionResolver {
    /// Resolves global (`~/.agent-studios/AGENTS.md|CLAUDE.md`) and hierarchical worktree
    /// instructions from `worktree_root` down to `active_cwd`, failing closed if `active_cwd`
    /// escapes `worktree_root`.
    pub fn resolve(
        global_home: Option<&Path>,
        worktree_root: &Path,
        active_cwd: &Path,
    ) -> Result<InstructionResolution, InstructionError> {
        let canon_root = dunce::canonicalize(worktree_root).map_err(|e| {
            InstructionError::Io(format!(
                "Cannot canonicalize worktree_root {}: {e}",
                worktree_root.display()
            ))
        })?;
        let canon_cwd = dunce::canonicalize(active_cwd).map_err(|e| {
            InstructionError::Io(format!(
                "Cannot canonicalize active_cwd {}: {e}",
                active_cwd.display()
            ))
        })?;

        if !canon_cwd.starts_with(&canon_root) {
            return Err(InstructionError::BoundaryViolation(format!(
                "active_cwd '{}' escapes worktree_root '{}'",
                canon_cwd.display(),
                canon_root.display()
            )));
        }

        let mut entries = Vec::new();

        // 1. Global home instructions (if configured)
        if let Some(home) = global_home {
            collect_dir_instructions(home, InstructionScope::Global, &mut entries)?;
        }

        // 2. Hierarchical path chain from worktree_root -> ... -> active_cwd
        let rel = canon_cwd.strip_prefix(&canon_root).map_err(|e| {
            InstructionError::BoundaryViolation(format!("Failed to compute relative path: {e}"))
        })?;

        let mut current = canon_root.clone();
        collect_dir_instructions(&current, InstructionScope::WorktreeRoot, &mut entries)?;

        for component in rel.components() {
            current.push(component);
            // Enforce symlink safety at each traversal segment
            if let Ok(meta) = fs::symlink_metadata(&current)
                && meta.file_type().is_symlink()
            {
                let target_canon = dunce::canonicalize(&current).map_err(|e| {
                    InstructionError::BoundaryViolation(format!(
                        "Broken symlink inside worktree: {e}"
                    ))
                })?;
                if !target_canon.starts_with(&canon_root) {
                    return Err(InstructionError::BoundaryViolation(format!(
                        "Symlink '{}' escapes worktree_root '{}'",
                        current.display(),
                        canon_root.display()
                    )));
                }
            }
            collect_dir_instructions(&current, InstructionScope::Directory, &mut entries)?;
        }

        Ok(InstructionResolution {
            worktree_root: canon_root,
            active_cwd: canon_cwd,
            entries,
        })
    }
}

fn collect_dir_instructions(
    dir: &Path,
    scope: InstructionScope,
    out: &mut Vec<InstructionEntry>,
) -> Result<(), InstructionError> {
    for (filename, kind) in [
        ("AGENTS.md", InstructionFileKind::AgentsMd),
        ("CLAUDE.md", InstructionFileKind::ClaudeMd),
    ] {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            if let Ok(meta) = fs::symlink_metadata(&candidate)
                && meta.file_type().is_symlink()
            {
                return Err(InstructionError::BoundaryViolation(format!(
                    "Symlink instruction file rejected: {}",
                    candidate.display()
                )));
            }
            let content = fs::read_to_string(&candidate).map_err(|e| {
                InstructionError::Io(format!("Failed to read {}: {e}", candidate.display()))
            })?;
            out.push(InstructionEntry {
                kind,
                scope,
                path: candidate,
                content,
            });
        }
    }
    Ok(())
}
