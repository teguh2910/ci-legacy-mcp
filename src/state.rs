use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleRecord {
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct StatusSummary {
    pub done: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub needs_review: usize,
    pub total_modules: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MigrationState {
    #[serde(default)]
    pub last_updated: String,
    #[serde(default)]
    pub class_mappings: BTreeMap<String, String>,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleRecord>,
}

impl MigrationState {
    pub fn state_file_path(workspace_root: &str) -> PathBuf {
        Path::new(workspace_root).join(".migration-state.json")
    }

    pub fn load_from_workspace(workspace_root: &str) -> Result<Self> {
        let path = Self::state_file_path(workspace_root);
        if !path.exists() {
            return Ok(Self {
                last_updated: Utc::now().to_rfc3339(),
                class_mappings: BTreeMap::new(),
                modules: BTreeMap::new(),
            });
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read state file at {}", path.display()))?;

        let state: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON state from {}", path.display()))?;

        Ok(state)
    }

    pub fn save_to_workspace(&self, workspace_root: &str) -> Result<()> {
        let path = Self::state_file_path(workspace_root);
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize migration state to JSON")?;

        fs::write(&path, json)
            .with_context(|| format!("Failed to write state file at {}", path.display()))?;

        Ok(())
    }

    pub fn compute_summary(&self) -> StatusSummary {
        let mut done = 0;
        let mut in_progress = 0;
        let mut pending = 0;
        let mut needs_review = 0;

        for record in self.modules.values() {
            match record.status.to_uppercase().as_str() {
                "DONE" | "COMPLETED" => done += 1,
                "IN_PROGRESS" | "INPROGRESS" => in_progress += 1,
                "NEEDS_REVIEW" | "NEEDSREVIEW" => needs_review += 1,
                _ => pending += 1,
            }
        }

        StatusSummary {
            done,
            in_progress,
            pending,
            needs_review,
            total_modules: self.modules.len(),
        }
    }
}
