use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use once_cell::sync::Lazy;
use regex::Regex;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

// Regex patterns for Controller
static RE_PUBLIC_METHOD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"public\s+function\s+([a-zA-Z0-9_]+)\s*\(([^)]*)\)"#).expect("valid regex")
});

static RE_CI3_INPUT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->input->(post|get|get_post)\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_SUPERGLOBALS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$_(POST|GET|FILES)\[['"]([^'"]+)['"]\]"#).expect("valid regex")
});

static RE_CI4_REQUEST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->request->get(Post|Get|Var|File)\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_SESSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->session->(userdata|set_userdata|flashdata|set_flashdata)\s*\(\s*['"]([^'"]+)['"]"#)
        .expect("valid regex")
});

static RE_CI4_SESSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:\$this->session->get|session)\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_VIEW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->view\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI4_VIEW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:return\s+)?view\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_LOAD_MODEL_SINGLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->model\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_LOAD_MODEL_ARRAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->model\s*\(\s*(?:array\(|\[)([^)\]]+)(?:\)|\])"#).expect("valid regex")
});

static RE_CI4_MODEL_USE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"use\s+App\\Models\\([a-zA-Z0-9_]+);"#).expect("valid regex")
});

static RE_CI4_MODEL_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"model\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_QUOTED_ITEM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"['"]([^'"]+)['"]"#).expect("valid regex")
});

// Regex patterns for Models
static RE_MODEL_METHOD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:public|protected|private)?\s*function\s+([a-zA-Z0-9_]+)\s*\(([^)]*)\)"#)
        .expect("valid regex")
});

static RE_QB_TABLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"->(?:get|from|join|insert|update|table)\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_RAW_QUERY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"->query\s*\("#).expect("valid regex")
});

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ModuleBlueprintRequest {
    /// Path root direktori proyek CodeIgniter.
    pub ci_root: String,
    /// Path relatif file Controller target (contoh: application/controllers/Inventory.php).
    pub controller_relative_path: String,
    /// Default true jika ukuran total file di bawah 800 baris, atau false jika hanya ingin ringkasan struktural.
    pub include_raw_code: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EndpointParameter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EndpointInfo {
    pub name: String,
    pub parameters: Vec<EndpointParameter>,
    pub raw_signature: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub model_name: String,
    pub file_path: Option<String>,
    pub methods: Vec<String>,
    pub tables_accessed: Vec<String>,
    pub has_raw_queries: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RawSourceFile {
    pub path: String,
    pub line_count: usize,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RawSources {
    pub controller: RawSourceFile,
    pub models: Vec<RawSourceFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ModuleBlueprintResponse {
    pub controller_path: String,
    pub endpoints: Vec<EndpointInfo>,
    pub inputs_detected: Vec<String>,
    pub session_keys: Vec<String>,
    pub linked_models: Vec<ModelInfo>,
    pub linked_views: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_sources: Option<RawSources>,
}

fn normalize_path_str(p: &str) -> String {
    p.replace('\\', "/")
}

fn normalize_path(path: &Path) -> String {
    normalize_path_str(&path.to_string_lossy())
}

fn parse_parameters(param_str: &str) -> Vec<EndpointParameter> {
    let trimmed = param_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for part in trimmed.split(',') {
        let part_trim = part.trim();
        if part_trim.is_empty() {
            continue;
        }

        if let Some((name_part, default_part)) = part_trim.split_once('=') {
            result.push(EndpointParameter {
                name: name_part.trim().to_string(),
                default_value: Some(default_part.trim().to_string()),
            });
        } else {
            result.push(EndpointParameter {
                name: part_trim.to_string(),
                default_value: None,
            });
        }
    }
    result
}

fn find_model_file(ci_root: &Path, model_name: &str) -> Option<PathBuf> {
    // Model name could include subpaths, e.g. "subfolder/User_model"
    let clean_model = model_name.trim_end_matches(".php");
    let model_basename = Path::new(clean_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(clean_model);

    // Direct checks
    let candidates = [
        ci_root.join("application").join("models").join(format!("{}.php", clean_model)),
        ci_root.join("application").join("models").join(format!("{}.php", clean_model.to_lowercase())),
        ci_root.join("app").join("Models").join(format!("{}.php", clean_model)),
        ci_root.join("app").join("Models").join(format!("{}.php", clean_model.to_lowercase())),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.is_file() {
            return Some(candidate.clone());
        }
    }

    // Walk models folder to find matching filename (case-insensitive)
    let models_dirs = [
        ci_root.join("application").join("models"),
        ci_root.join("app").join("Models"),
    ];

    for models_dir in &models_dirs {
        if !models_dir.exists() {
            continue;
        }
        for entry in WalkDir::new(models_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let name = entry.file_name().to_string_lossy();
                let stem = name.trim_end_matches(".php");
                if stem.eq_ignore_ascii_case(model_basename) {
                    return Some(entry.into_path());
                }
            }
        }
    }

    None
}

fn verify_view_path(ci_root: &Path, view_name: &str) -> String {
    let clean_view = view_name.trim_matches('/');
    let with_ext = if clean_view.ends_with(".php") {
        clean_view.to_string()
    } else {
        format!("{}.php", clean_view)
    };

    let candidates = [
        ci_root.join("application").join("views").join(&with_ext),
        ci_root.join("app").join("Views").join(&with_ext),
        ci_root.join("views").join(&with_ext),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            if let Ok(rel) = candidate.strip_prefix(ci_root) {
                return normalize_path(rel);
            }
            return normalize_path(candidate);
        }
    }

    // If not found, still return the expected relative path noted as not found
    format!("application/views/{} (not found on disk)", with_ext)
}

pub fn extract_blueprint(
    ci_root: &Path,
    controller_rel_path: &str,
    include_raw_code: Option<bool>,
) -> Result<ModuleBlueprintResponse, String> {
    if !ci_root.exists() {
        return Err(format!("ci_root '{}' does not exist.", ci_root.display()));
    }

    let clean_rel = controller_rel_path.trim().trim_start_matches('/');
    let controller_full_path = ci_root.join(clean_rel);

    if !controller_full_path.exists() {
        return Err(format!(
            "Controller file '{}' does not exist in '{}'.",
            clean_rel,
            ci_root.display()
        ));
    }

    let controller_content = fs::read_to_string(&controller_full_path)
        .map_err(|e| format!("Failed to read controller file '{}': {}", clean_rel, e))?;

    let controller_line_count = controller_content.lines().count();

    // 1. Extract Endpoints
    let mut endpoints = Vec::new();
    for cap in RE_PUBLIC_METHOD.captures_iter(&controller_content) {
        if let Some(m_name) = cap.get(1) {
            let name = m_name.as_str();
            if name != "__construct" {
                let params_raw = cap.get(2).map(|p| p.as_str()).unwrap_or("");
                endpoints.push(EndpointInfo {
                    name: name.to_string(),
                    parameters: parse_parameters(params_raw),
                    raw_signature: format!("function {}({})", name, params_raw),
                });
            }
        }
    }

    // 2. Extract HTTP Inputs
    let mut inputs_set = BTreeSet::new();
    for cap in RE_CI3_INPUT.captures_iter(&controller_content) {
        if let (Some(m_type), Some(m_key)) = (cap.get(1), cap.get(2)) {
            inputs_set.insert(format!("{}: {}", m_type.as_str().to_uppercase(), m_key.as_str()));
        }
    }
    for cap in RE_SUPERGLOBALS.captures_iter(&controller_content) {
        if let (Some(m_type), Some(m_key)) = (cap.get(1), cap.get(2)) {
            inputs_set.insert(format!("{}: {}", m_type.as_str().to_uppercase(), m_key.as_str()));
        }
    }
    for cap in RE_CI4_REQUEST.captures_iter(&controller_content) {
        if let (Some(m_type), Some(m_key)) = (cap.get(1), cap.get(2)) {
            inputs_set.insert(format!("{}: {}", m_type.as_str().to_uppercase(), m_key.as_str()));
        }
    }
    let inputs_detected: Vec<String> = inputs_set.into_iter().collect();

    // 3. Extract Session Keys
    let mut session_set = BTreeSet::new();
    for cap in RE_CI3_SESSION.captures_iter(&controller_content) {
        if let Some(m_key) = cap.get(2) {
            session_set.insert(m_key.as_str().to_string());
        }
    }
    for cap in RE_CI4_SESSION.captures_iter(&controller_content) {
        if let Some(m_key) = cap.get(1) {
            session_set.insert(m_key.as_str().to_string());
        }
    }
    let session_keys: Vec<String> = session_set.into_iter().collect();

    // 4. Extract Rendered Views
    let mut view_names_set = BTreeSet::new();
    for cap in RE_CI3_VIEW.captures_iter(&controller_content) {
        if let Some(m_view) = cap.get(1) {
            view_names_set.insert(m_view.as_str().to_string());
        }
    }
    for cap in RE_CI4_VIEW.captures_iter(&controller_content) {
        if let Some(m_view) = cap.get(1) {
            view_names_set.insert(m_view.as_str().to_string());
        }
    }
    let linked_views: Vec<String> = view_names_set
        .into_iter()
        .map(|v| verify_view_path(ci_root, &v))
        .collect();

    // 5. Extract Loaded Models
    let mut model_names_set = BTreeSet::new();
    for cap in RE_CI3_LOAD_MODEL_SINGLE.captures_iter(&controller_content) {
        if let Some(m) = cap.get(1) {
            let m_str = m.as_str().trim();
            if !m_str.is_empty() {
                model_names_set.insert(m_str.to_string());
            }
        }
    }
    for cap in RE_CI3_LOAD_MODEL_ARRAY.captures_iter(&controller_content) {
        if let Some(inner) = cap.get(1) {
            for item in RE_QUOTED_ITEM.captures_iter(inner.as_str()) {
                if let Some(m) = item.get(1) {
                    let m_str = m.as_str().trim();
                    if !m_str.is_empty() {
                        model_names_set.insert(m_str.to_string());
                    }
                }
            }
        }
    }
    for cap in RE_CI4_MODEL_USE.captures_iter(&controller_content) {
        if let Some(m) = cap.get(1) {
            model_names_set.insert(m.as_str().to_string());
        }
    }
    for cap in RE_CI4_MODEL_FUNC.captures_iter(&controller_content) {
        if let Some(m) = cap.get(1) {
            model_names_set.insert(m.as_str().to_string());
        }
    }

    // Inspect each linked model
    let mut linked_models = Vec::new();
    let mut model_raw_sources = Vec::new();
    let mut total_lines = controller_line_count;

    for model_name in model_names_set {
        let model_file_opt = find_model_file(ci_root, &model_name);
        if let Some(model_file) = model_file_opt {
            let rel_model_path = model_file
                .strip_prefix(ci_root)
                .map(|p| normalize_path(p))
                .unwrap_or_else(|_| normalize_path(&model_file));

            if let Ok(model_code) = fs::read_to_string(&model_file) {
                let m_lines = model_code.lines().count();
                total_lines += m_lines;

                // Extract methods
                let mut methods_set = BTreeSet::new();
                for cap in RE_MODEL_METHOD.captures_iter(&model_code) {
                    if let Some(fn_name) = cap.get(1) {
                        let name_str = fn_name.as_str();
                        if name_str != "__construct" {
                            methods_set.insert(name_str.to_string());
                        }
                    }
                }

                // Extract tables accessed
                let mut tables_set = BTreeSet::new();
                for cap in RE_QB_TABLE.captures_iter(&model_code) {
                    if let Some(tbl) = cap.get(1) {
                        tables_set.insert(tbl.as_str().to_string());
                    }
                }

                let has_raw_queries = RE_RAW_QUERY.is_match(&model_code);

                linked_models.push(ModelInfo {
                    model_name: model_name.clone(),
                    file_path: Some(rel_model_path.clone()),
                    methods: methods_set.into_iter().collect(),
                    tables_accessed: tables_set.into_iter().collect(),
                    has_raw_queries,
                });

                model_raw_sources.push(RawSourceFile {
                    path: rel_model_path,
                    line_count: m_lines,
                    content: model_code,
                });
            } else {
                linked_models.push(ModelInfo {
                    model_name,
                    file_path: Some(rel_model_path),
                    methods: Vec::new(),
                    tables_accessed: Vec::new(),
                    has_raw_queries: false,
                });
            }
        } else {
            linked_models.push(ModelInfo {
                model_name,
                file_path: None,
                methods: Vec::new(),
                tables_accessed: Vec::new(),
                has_raw_queries: false,
            });
        }
    }

    let should_include_raw = include_raw_code.unwrap_or(total_lines < 800);

    let raw_sources = if should_include_raw {
        Some(RawSources {
            controller: RawSourceFile {
                path: clean_rel.to_string(),
                line_count: controller_line_count,
                content: controller_content,
            },
            models: model_raw_sources,
        })
    } else {
        None
    };

    Ok(ModuleBlueprintResponse {
        controller_path: clean_rel.to_string(),
        endpoints,
        inputs_detected,
        session_keys,
        linked_models,
        linked_views,
        raw_sources,
    })
}

pub async fn handle_extract_module_blueprint(req: ModuleBlueprintRequest) -> CallToolResult {
    let ci_root_path = PathBuf::from(req.ci_root.trim());
    match extract_blueprint(&ci_root_path, &req.controller_relative_path, req.include_raw_code) {
        Ok(resp) => match serde_json::to_value(&resp) {
            Ok(json_val) => CallToolResult::structured(json_val),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to serialize module blueprint response: {}",
                e
            ))]),
        },
        Err(err_msg) => CallToolResult::error(vec![ContentBlock::text(err_msg)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_extract_module_blueprint_success() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let controllers_dir = root.join("application").join("controllers");
        let models_dir = root.join("application").join("models");
        let views_dir = root.join("application").join("views").join("inventory");

        fs::create_dir_all(&controllers_dir).unwrap();
        fs::create_dir_all(&models_dir).unwrap();
        fs::create_dir_all(&views_dir).unwrap();

        // Create Controller
        let mut f_ctrl = File::create(controllers_dir.join("Inventory.php")).unwrap();
        writeln!(
            f_ctrl,
            r#"<?php
class Inventory extends CI_Controller {{
    public function __construct() {{
        parent::__construct();
        $this->load->model('Item_model');
    }}

    public function index($page = 1) {{
        $search = $this->input->get('q');
        $role = $this->session->userdata('user_role');
        $this->load->view('inventory/list');
    }}

    public function store() {{
        $name = $this->input->post('item_name');
        $file = $_FILES['attachment'];
        $this->session->set_flashdata('success', 'Item created');
    }}
}}
"#
        )
        .unwrap();

        // Create Model
        let mut f_model = File::create(models_dir.join("Item_model.php")).unwrap();
        writeln!(
            f_model,
            r#"<?php
class Item_model extends CI_Model {{
    public function get_items() {{
        return $this->db->get('items')->result_array();
    }}

    public function add_item($data) {{
        $this->db->insert('items', $data);
        $this->db->join('categories', 'categories.id = items.category_id');
        return $this->db->query("SELECT LAST_INSERT_ID()");
    }}
}}
"#
        )
        .unwrap();

        // Create View
        File::create(views_dir.join("list.php")).unwrap();

        let result = extract_blueprint(root, "application/controllers/Inventory.php", Some(true))
            .expect("should extract successfully");

        assert_eq!(result.controller_path, "application/controllers/Inventory.php");
        assert_eq!(result.endpoints.len(), 2);
        assert_eq!(result.endpoints[0].name, "index");
        assert_eq!(result.endpoints[0].parameters.len(), 1);
        assert_eq!(result.endpoints[0].parameters[0].name, "$page");
        assert_eq!(result.endpoints[0].parameters[0].default_value.as_deref(), Some("1"));
        assert_eq!(result.endpoints[1].name, "store");

        // HTTP inputs
        assert!(result.inputs_detected.contains(&"GET: q".to_string()));
        assert!(result.inputs_detected.contains(&"POST: item_name".to_string()));
        assert!(result.inputs_detected.contains(&"FILES: attachment".to_string()));

        // Session
        assert!(result.session_keys.contains(&"user_role".to_string()));
        assert!(result.session_keys.contains(&"success".to_string()));

        // Views
        assert_eq!(result.linked_views.len(), 1);
        assert_eq!(result.linked_views[0], "application/views/inventory/list.php");

        // Models
        assert_eq!(result.linked_models.len(), 1);
        let m = &result.linked_models[0];
        assert_eq!(m.model_name, "Item_model");
        assert!(m.methods.contains(&"get_items".to_string()));
        assert!(m.methods.contains(&"add_item".to_string()));
        assert!(m.tables_accessed.contains(&"items".to_string()));
        assert!(m.tables_accessed.contains(&"categories".to_string()));
        assert!(m.has_raw_queries);

        // Raw code
        assert!(result.raw_sources.is_some());
    }

    #[test]
    fn test_extract_module_blueprint_missing_controller() {
        let dir = tempdir().unwrap();
        let result = extract_blueprint(dir.path(), "application/controllers/Unknown.php", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }
}
