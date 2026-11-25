//! Skills Engine (`SKILL.md`) for Agent Studios (M16)
//!
//! Supports:
//! - Orthogonal Scope (`Global`, `Project`, `Agent`) vs Provenance (`Bundled`, `LocalFolder`, `GitHub`, `Registry`)
//! - Progressive disclosure (summary metadata loaded at baseline; full markdown loaded on demand)
//! - Safe local installation with path traversal and symlink rejection (never auto-executes scripts)
//! - Enable/disable persistence and dependency validation

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Project,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillProvenance {
    Bundled,
    LocalFolder {
        path: String,
    },
    GitHub {
        repo_url: String,
        commit_sha: Option<String>,
    },
    Registry {
        registry_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    #[serde(default = "default_skill_version")]
    pub version: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
}

fn default_skill_version() -> String {
    "0.1.0".to_string()
}

/// Lightweight summary loaded into baseline prompts (progressive disclosure stage 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub enabled: bool,
    pub triggers: Vec<String>,
}

/// Registered skill descriptor pointing to `SKILL.md` on disk without preloading its full body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub manifest: SkillManifest,
    pub scope: SkillScope,
    pub provenance: SkillProvenance,
    pub skill_dir: PathBuf,
    pub skill_file: PathBuf,
    pub enabled: bool,
}

impl SkillDescriptor {
    /// Returns only metadata & trigger summaries (never reads full instructions body).
    pub fn summary(&self) -> SkillSummary {
        SkillSummary {
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            scope: self.scope,
            enabled: self.enabled,
            triggers: self.manifest.triggers.clone(),
        }
    }

    /// Progressively loads the full markdown instruction body on demand.
    pub fn load_full_instructions(&self) -> Result<String, SkillError> {
        let raw = fs::read_to_string(&self.skill_file).map_err(|e| {
            SkillError::Io(format!("Failed to read {}: {e}", self.skill_file.display()))
        })?;
        let (_, body) = split_frontmatter(&raw)?;
        Ok(body.trim().to_string())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SkillError {
    #[error("Invalid SKILL.md format: {0}")]
    InvalidFormat(String),
    #[error("Skill security violation: {0}")]
    SecurityViolation(String),
    #[error("Skill not found: {0}")]
    NotFound(String),
    #[error("Missing skill dependency '{dependency}' required by '{skill}'")]
    MissingDependency { skill: String, dependency: String },
    #[error("I/O error: {0}")]
    Io(String),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillStateFile {
    #[serde(default)]
    disabled: HashSet<String>,
    #[serde(default)]
    provenance: HashMap<String, SkillProvenance>,
}

/// Parses YAML-subset frontmatter and markdown body from a `SKILL.md` file.
pub fn parse_skill_md(raw: &str) -> Result<(SkillManifest, String), SkillError> {
    let (frontmatter, body) = split_frontmatter(raw)?;

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut version = default_skill_version();
    let mut tools = Vec::new();
    let mut dependencies = Vec::new();
    let mut triggers = Vec::new();
    let mut active_list: Option<&str> = None;

    for raw_line in frontmatter.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("- ") {
            let val = unquote(item.trim());
            match active_list {
                Some("tools") => tools.push(val),
                Some("dependencies") => dependencies.push(val),
                Some("triggers") => triggers.push(val),
                _ => {}
            }
            continue;
        }

        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            active_list = None;
            match key {
                "name" => name = Some(unquote(val)),
                "description" => description = Some(unquote(val)),
                "version" => {
                    let parsed = unquote(val);
                    if !parsed.is_empty() {
                        version = parsed;
                    }
                }
                "tools" => {
                    if val.is_empty() {
                        active_list = Some("tools");
                    } else {
                        tools.extend(parse_inline_list(val));
                    }
                }
                "dependencies" => {
                    if val.is_empty() {
                        active_list = Some("dependencies");
                    } else {
                        dependencies.extend(parse_inline_list(val));
                    }
                }
                "triggers" => {
                    if val.is_empty() {
                        active_list = Some("triggers");
                    } else {
                        triggers.extend(parse_inline_list(val));
                    }
                }
                _ => {}
            }
        }
    }

    let name = name
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SkillError::InvalidFormat("Missing 'name' in frontmatter".to_string()))?;
    let description = description.unwrap_or_default();

    Ok((
        SkillManifest {
            name,
            description,
            version,
            tools,
            dependencies,
            triggers,
        },
        body.to_string(),
    ))
}

fn split_frontmatter(raw: &str) -> Result<(&str, &str), SkillError> {
    let normalized = raw.trim_start_matches('\u{feff}');
    if !normalized.starts_with("---") {
        return Err(SkillError::InvalidFormat(
            "SKILL.md must start with YAML frontmatter '---'".to_string(),
        ));
    }
    let after_open = &normalized[3..];
    let after_open = after_open
        .strip_prefix("\r\n")
        .or_else(|| after_open.strip_prefix('\n'))
        .ok_or_else(|| {
            SkillError::InvalidFormat("Malformed opening frontmatter delimiter".to_string())
        })?;

    if let Some(idx) = after_open.find("\n---") {
        let fm = &after_open[..idx];
        let rest = &after_open[idx + 4..];
        let rest = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix('\n'))
            .unwrap_or(rest);
        Ok((fm, rest))
    } else if let Some(idx) = after_open.find("\r\n---") {
        let fm = &after_open[..idx];
        let rest = &after_open[idx + 5..];
        let rest = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix('\n'))
            .unwrap_or(rest);
        Ok((fm, rest))
    } else {
        Err(SkillError::InvalidFormat(
            "Closing YAML frontmatter delimiter '---' not found".to_string(),
        ))
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
        && t.len() >= 2
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn parse_inline_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed[1..trimmed.len() - 1]
            .split(',')
            .map(unquote)
            .filter(|s| !s.is_empty())
            .collect()
    } else if !trimmed.is_empty() {
        vec![unquote(trimmed)]
    } else {
        Vec::new()
    }
}

/// Manages discovery, enable/disable state, dependency checks, and safe installation of skills.
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    global_root: PathBuf,
    project_root: Option<PathBuf>,
    agent_id: Option<String>,
    skills: BTreeMap<String, SkillDescriptor>,
}

impl SkillRegistry {
    pub fn new(global_home: impl Into<PathBuf>, project_root: Option<PathBuf>) -> Self {
        Self {
            global_root: global_home.into(),
            project_root,
            agent_id: None,
            skills: BTreeMap::new(),
        }
    }

    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn global_skills_dir(&self) -> PathBuf {
        self.global_root.join("skills")
    }

    pub fn project_skills_dir(&self) -> Option<PathBuf> {
        self.project_root
            .as_ref()
            .map(|p| p.join(".agent-studios").join("skills"))
    }

    pub fn agent_skills_dir(&self) -> Option<PathBuf> {
        match (&self.project_root, &self.agent_id) {
            (Some(p), Some(a)) => Some(
                p.join(".agent-studios")
                    .join("agents")
                    .join(a)
                    .join("skills"),
            ),
            _ => None,
        }
    }

    fn state_file_path(&self) -> PathBuf {
        self.global_root.join("skills-state.json")
    }

    fn load_state(&self) -> SkillStateFile {
        let p = self.state_file_path();
        if let Ok(raw) = fs::read_to_string(p)
            && let Ok(state) = serde_json::from_str::<SkillStateFile>(&raw)
        {
            return state;
        }
        SkillStateFile::default()
    }

    fn save_state(&self, state: &SkillStateFile) -> Result<(), SkillError> {
        fs::create_dir_all(&self.global_root)
            .map_err(|e| SkillError::Io(format!("Failed to create global root: {e}")))?;
        let raw = serde_json::to_string_pretty(state)
            .map_err(|e| SkillError::Io(format!("Failed to serialize skill state: {e}")))?;
        fs::write(self.state_file_path(), raw)
            .map_err(|e| SkillError::Io(format!("Failed to write skill state: {e}")))
    }

    /// Discovers skills across Global, Project, and Agent scopes (Agent overrides Project overrides Global).
    pub fn discover(&mut self) -> Result<Vec<SkillDescriptor>, SkillError> {
        self.skills.clear();
        let state = self.load_state();

        let mut scope_dirs = vec![(SkillScope::Global, self.global_skills_dir())];
        if let Some(proj) = self.project_skills_dir() {
            scope_dirs.push((SkillScope::Project, proj));
        }
        if let Some(agent) = self.agent_skills_dir() {
            scope_dirs.push((SkillScope::Agent, agent));
        }

        for (scope, dir) in scope_dirs {
            if !dir.is_dir() {
                continue;
            }
            let entries = fs::read_dir(&dir)
                .map_err(|e| SkillError::Io(format!("Failed to scan {}: {e}", dir.display())))?;
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(meta) = fs::symlink_metadata(&path)
                    && meta.file_type().is_symlink()
                {
                    continue;
                }
                if !path.is_dir() {
                    continue;
                }
                let skill_file = path.join("SKILL.md");
                if !skill_file.is_file() {
                    continue;
                }
                if let Ok(meta) = fs::symlink_metadata(&skill_file)
                    && meta.file_type().is_symlink()
                {
                    continue;
                }
                let raw = fs::read_to_string(&skill_file).map_err(|e| {
                    SkillError::Io(format!("Failed to read {}: {e}", skill_file.display()))
                })?;
                let (manifest, _) = parse_skill_md(&raw)?;
                let key = manifest.name.clone();
                let enabled = !state.disabled.contains(&key);
                let provenance = state.provenance.get(&key).cloned().unwrap_or_else(|| {
                    SkillProvenance::LocalFolder {
                        path: path.to_string_lossy().to_string(),
                    }
                });

                self.skills.insert(
                    key,
                    SkillDescriptor {
                        manifest,
                        scope,
                        provenance,
                        skill_dir: path,
                        skill_file,
                        enabled,
                    },
                );
            }
        }

        Ok(self.list())
    }

    pub fn list(&self) -> Vec<SkillDescriptor> {
        self.skills.values().cloned().collect()
    }

    pub fn summaries(&self) -> Vec<SkillSummary> {
        self.skills
            .values()
            .filter(|s| s.enabled)
            .map(SkillDescriptor::summary)
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&SkillDescriptor> {
        self.skills.get(name)
    }

    /// Safely installs a skill from a local folder into the requested scope without
    /// executing any scripts inside the skill repository and rejecting symlinks/traversal.
    pub fn install_from_local_dir(
        &mut self,
        source_dir: &Path,
        scope: SkillScope,
        provenance: SkillProvenance,
    ) -> Result<SkillDescriptor, SkillError> {
        let skill_md = source_dir.join("SKILL.md");
        if !skill_md.is_file() {
            return Err(SkillError::InvalidFormat(format!(
                "No SKILL.md found in {}",
                source_dir.display()
            )));
        }

        // Security gate: reject symlinks inside source skill package
        validate_no_symlinks_recursive(source_dir)?;

        let raw = fs::read_to_string(&skill_md)
            .map_err(|e| SkillError::Io(format!("Failed to read {}: {e}", skill_md.display())))?;
        let (manifest, _) = parse_skill_md(&raw)?;

        if manifest.name.contains("..")
            || manifest.name.contains('/')
            || manifest.name.contains('\\')
        {
            return Err(SkillError::SecurityViolation(format!(
                "Invalid skill name '{}': path separators/traversal not allowed",
                manifest.name
            )));
        }

        let target_base = match scope {
            SkillScope::Global => self.global_skills_dir(),
            SkillScope::Project => self.project_skills_dir().ok_or_else(|| {
                SkillError::InvalidFormat(
                    "No project root configured for Project scope".to_string(),
                )
            })?,
            SkillScope::Agent => self.agent_skills_dir().ok_or_else(|| {
                SkillError::InvalidFormat(
                    "Project root and agent ID required for Agent scope".to_string(),
                )
            })?,
        };

        fs::create_dir_all(&target_base)
            .map_err(|e| SkillError::Io(format!("Failed to create target skill root: {e}")))?;

        let dest_dir = target_base.join(&manifest.name);
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir)
                .map_err(|e| SkillError::Io(format!("Failed to replace existing skill: {e}")))?;
        }
        copy_dir_non_executable_safe(source_dir, &dest_dir)?;

        let mut state = self.load_state();
        state
            .provenance
            .insert(manifest.name.clone(), provenance.clone());
        self.save_state(&state)?;

        let descriptor = SkillDescriptor {
            enabled: !state.disabled.contains(&manifest.name),
            manifest,
            scope,
            provenance,
            skill_file: dest_dir.join("SKILL.md"),
            skill_dir: dest_dir,
        };
        self.skills
            .insert(descriptor.manifest.name.clone(), descriptor.clone());
        Ok(descriptor)
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), SkillError> {
        let Some(skill) = self.skills.get_mut(name) else {
            return Err(SkillError::NotFound(name.to_string()));
        };
        skill.enabled = enabled;
        let mut state = self.load_state();
        if enabled {
            state.disabled.remove(name);
        } else {
            state.disabled.insert(name.to_string());
        }
        self.save_state(&state)
    }

    /// Validates that every enabled skill has all declared dependencies enabled and present.
    pub fn validate_dependencies(&self) -> Result<(), SkillError> {
        for skill in self.skills.values() {
            if !skill.enabled {
                continue;
            }
            for dep in &skill.manifest.dependencies {
                match self.skills.get(dep) {
                    Some(d) if d.enabled => {}
                    _ => {
                        return Err(SkillError::MissingDependency {
                            skill: skill.manifest.name.clone(),
                            dependency: dep.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_no_symlinks_recursive(dir: &Path) -> Result<(), SkillError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| SkillError::Io(format!("Failed to scan {}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let p = entry.path();
        let meta = fs::symlink_metadata(&p)
            .map_err(|e| SkillError::Io(format!("Failed to stat {}: {e}", p.display())))?;
        if meta.file_type().is_symlink() {
            return Err(SkillError::SecurityViolation(format!(
                "Symlink rejected inside untrusted skill package: {}",
                p.display()
            )));
        }
        if meta.is_dir() {
            validate_no_symlinks_recursive(&p)?;
        }
    }
    Ok(())
}

fn copy_dir_non_executable_safe(src: &Path, dst: &Path) -> Result<(), SkillError> {
    fs::create_dir_all(dst)
        .map_err(|e| SkillError::Io(format!("Failed to create dir {}: {e}", dst.display())))?;
    for entry in fs::read_dir(src)
        .map_err(|e| SkillError::Io(format!("Failed to read dir {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| SkillError::Io(e.to_string()))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&src_path).map_err(|e| SkillError::Io(e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(SkillError::SecurityViolation(format!(
                "Symlink rejected during skill copy: {}",
                src_path.display()
            )));
        }
        if meta.is_dir() {
            copy_dir_non_executable_safe(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                SkillError::Io(format!("Failed to copy {}: {e}", src_path.display()))
            })?;
        }
    }
    Ok(())
}
