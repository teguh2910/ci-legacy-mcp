use std::path::Path;
use chrono::Utc;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::state::{MigrationState, ModuleRecord, StatusSummary};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TrackerRequest {
    /// Path root workspace proyek.
    pub workspace_root: String,
    /// Salah satu dari "get_state", "update_module", atau "register_mapping".
    pub action: String,
    /// Nama modul/controller (wajib untuk update_module).
    pub module_name: Option<String>,
    /// Status baru: "PENDING", "IN_PROGRESS", "DONE", atau "NEEDS_REVIEW".
    pub status: Option<String>,
    /// Catatan teknis atau keterangan modul.
    pub notes: Option<String>,
    /// Daftar file Laravel yang dihasilkan (opsional).
    pub generated_files: Option<Vec<String>>,
    /// Nama class/file lama di CI (misal: M_barang atau ion_auth).
    pub legacy_name: Option<String>,
    /// Nama class baru di Laravel (misal: App\Models\Product atau App\Services\AuthService).
    pub laravel_fqcn: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct GetStateResponse {
    pub message: String,
    pub status_summary: StatusSummary,
    pub state: MigrationState,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateModuleResponse {
    pub message: String,
    pub module_name: String,
    pub updated_record: ModuleRecord,
    pub status_summary: StatusSummary,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RegisterMappingResponse {
    pub message: String,
    pub legacy_name: String,
    pub laravel_fqcn: String,
    pub total_mappings: usize,
    pub class_mappings: std::collections::BTreeMap<String, String>,
}

pub fn execute_tracker_action(req: &TrackerRequest) -> Result<serde_json::Value, String> {
    let workspace_path = Path::new(&req.workspace_root);
    if !workspace_path.exists() {
        return Err(format!("workspace_root '{}' does not exist.", req.workspace_root));
    }
    if !workspace_path.is_dir() {
        return Err(format!("workspace_root '{}' is not a directory.", req.workspace_root));
    }

    let mut state = MigrationState::load_from_workspace(&req.workspace_root)
        .map_err(|e| format!("Failed to load migration state: {}", e))?;

    match req.action.to_lowercase().as_str() {
        "get_state" => {
            let state_file = MigrationState::state_file_path(&req.workspace_root);
            if !state_file.exists() {
                let _ = state.save_to_workspace(&req.workspace_root);
            }
            let summary = state.compute_summary();
            let response = GetStateResponse {
                message: format!(
                    "Retrieved migration state with {} total modules ({} DONE, {} IN_PROGRESS, {} PENDING)",
                    summary.total_modules, summary.done, summary.in_progress, summary.pending
                ),
                status_summary: summary,
                state,
            };
            serde_json::to_value(&response).map_err(|e| format!("JSON error: {}", e))
        }

        "update_module" => {
            let module_name = match &req.module_name {
                Some(name) if !name.trim().is_empty() => name.trim().to_string(),
                _ => return Err("Parameter 'module_name' is required for action 'update_module'.".to_string()),
            };

            let mut record = state.modules.get(&module_name).cloned().unwrap_or_default();

            if let Some(st) = &req.status {
                record.status = st.trim().to_uppercase();
            } else if record.status.is_empty() {
                record.status = "IN_PROGRESS".to_string();
            }

            if let Some(nt) = &req.notes {
                record.notes = Some(nt.clone());
            }

            if let Some(files) = &req.generated_files {
                for file in files {
                    if !record.generated_files.contains(file) {
                        record.generated_files.push(file.clone());
                    }
                }
            }

            state.last_updated = Utc::now().to_rfc3339();
            state.modules.insert(module_name.clone(), record.clone());

            state.save_to_workspace(&req.workspace_root)
                .map_err(|e| format!("Failed to save migration state: {}", e))?;

            let summary = state.compute_summary();
            let response = UpdateModuleResponse {
                message: format!("Successfully updated module '{}' to status '{}'.", module_name, record.status),
                module_name,
                updated_record: record,
                status_summary: summary,
            };
            serde_json::to_value(&response).map_err(|e| format!("JSON error: {}", e))
        }

        "register_mapping" => {
            let legacy = match &req.legacy_name {
                Some(name) if !name.trim().is_empty() => name.trim().to_string(),
                _ => return Err("Parameter 'legacy_name' is required for action 'register_mapping'.".to_string()),
            };
            let laravel = match &req.laravel_fqcn {
                Some(name) if !name.trim().is_empty() => name.trim().to_string(),
                _ => return Err("Parameter 'laravel_fqcn' is required for action 'register_mapping'.".to_string()),
            };

            state.class_mappings.insert(legacy.clone(), laravel.clone());
            state.last_updated = Utc::now().to_rfc3339();

            state.save_to_workspace(&req.workspace_root)
                .map_err(|e| format!("Failed to save migration state: {}", e))?;

            let total_mappings = state.class_mappings.len();
            let response = RegisterMappingResponse {
                message: format!("Successfully registered mapping '{}' -> '{}'.", legacy, laravel),
                legacy_name: legacy,
                laravel_fqcn: laravel,
                total_mappings,
                class_mappings: state.class_mappings,
            };
            serde_json::to_value(&response).map_err(|e| format!("JSON error: {}", e))
        }

        other => Err(format!(
            "Unknown action '{}'. Valid actions are: 'get_state', 'update_module', 'register_mapping'.",
            other
        )),
    }
}

pub async fn handle_track_migration_state(req: TrackerRequest) -> CallToolResult {
    match execute_tracker_action(&req) {
        Ok(json_val) => CallToolResult::structured(json_val),
        Err(err_msg) => CallToolResult::error(vec![ContentBlock::text(err_msg)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tracker_lifecycle() {
        let dir = tempdir().unwrap();
        let ws = dir.path().to_str().unwrap().to_string();

        // 1. Initial get_state
        let req_get = TrackerRequest {
            workspace_root: ws.clone(),
            action: "get_state".to_string(),
            module_name: None,
            status: None,
            notes: None,
            generated_files: None,
            legacy_name: None,
            laravel_fqcn: None,
        };
        let res_get = execute_tracker_action(&req_get).expect("get_state should succeed");
        let resp_get: GetStateResponse = serde_json::from_value(res_get).unwrap();
        assert_eq!(resp_get.status_summary.total_modules, 0);

        // 2. Register mapping
        let req_map = TrackerRequest {
            workspace_root: ws.clone(),
            action: "register_mapping".to_string(),
            module_name: None,
            status: None,
            notes: None,
            generated_files: None,
            legacy_name: Some("M_barang".to_string()),
            laravel_fqcn: Some("App\\Models\\Product".to_string()),
        };
        let res_map = execute_tracker_action(&req_map).expect("register_mapping should succeed");
        let resp_map: RegisterMappingResponse = serde_json::from_value(res_map).unwrap();
        assert_eq!(resp_map.total_mappings, 1);
        assert_eq!(resp_map.class_mappings.get("M_barang").unwrap(), "App\\Models\\Product");

        // 3. Update module to IN_PROGRESS
        let req_mod1 = TrackerRequest {
            workspace_root: ws.clone(),
            action: "update_module".to_string(),
            module_name: Some("application/controllers/Auth.php".to_string()),
            status: Some("IN_PROGRESS".to_string()),
            notes: Some("Started migration".to_string()),
            generated_files: Some(vec!["app/Http/Controllers/AuthController.php".to_string()]),
            legacy_name: None,
            laravel_fqcn: None,
        };
        let res_mod1 = execute_tracker_action(&req_mod1).expect("update_module should succeed");
        let resp_mod1: UpdateModuleResponse = serde_json::from_value(res_mod1).unwrap();
        assert_eq!(resp_mod1.updated_record.status, "IN_PROGRESS");
        assert_eq!(resp_mod1.status_summary.in_progress, 1);

        // 4. Update module to DONE
        let req_mod2 = TrackerRequest {
            workspace_root: ws.clone(),
            action: "update_module".to_string(),
            module_name: Some("application/controllers/Auth.php".to_string()),
            status: Some("DONE".to_string()),
            notes: Some("Session CI diganti ke Sanctum".to_string()),
            generated_files: Some(vec!["app/Http/Requests/LoginRequest.php".to_string()]),
            legacy_name: None,
            laravel_fqcn: None,
        };
        let res_mod2 = execute_tracker_action(&req_mod2).expect("update_module should succeed");
        let resp_mod2: UpdateModuleResponse = serde_json::from_value(res_mod2).unwrap();
        assert_eq!(resp_mod2.updated_record.status, "DONE");
        assert_eq!(resp_mod2.updated_record.generated_files.len(), 2);
        assert_eq!(resp_mod2.status_summary.done, 1);
        assert_eq!(resp_mod2.status_summary.in_progress, 0);

        // 5. Verify state file persisted on disk
        let state_on_disk = MigrationState::load_from_workspace(&ws).unwrap();
        assert_eq!(state_on_disk.modules.len(), 1);
        assert_eq!(state_on_disk.class_mappings.len(), 1);
    }

    #[test]
    fn test_tracker_validation_errors() {
        let dir = tempdir().unwrap();
        let ws = dir.path().to_str().unwrap().to_string();

        let req_invalid_action = TrackerRequest {
            workspace_root: ws.clone(),
            action: "invalid_action".to_string(),
            module_name: None,
            status: None,
            notes: None,
            generated_files: None,
            legacy_name: None,
            laravel_fqcn: None,
        };
        assert!(execute_tracker_action(&req_invalid_action).is_err());

        let req_missing_module = TrackerRequest {
            workspace_root: ws,
            action: "update_module".to_string(),
            module_name: None,
            status: Some("DONE".to_string()),
            notes: None,
            generated_files: None,
            legacy_name: None,
            laravel_fqcn: None,
        };
        assert!(execute_tracker_action(&req_missing_module).is_err());
    }
}
