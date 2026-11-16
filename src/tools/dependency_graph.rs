use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use once_cell::sync::Lazy;
use regex::Regex;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

static RE_CI3_MODEL_SINGLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->model\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_MODEL_ARRAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->model\s*\(\s*(?:array\(|\[)([^)\]]+)(?:\)|\])"#)
        .expect("valid regex")
});

static RE_CI3_LIBRARY_SINGLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->library\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_LIBRARY_ARRAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->library\s*\(\s*(?:array\(|\[)([^)\]]+)(?:\)|\])"#)
        .expect("valid regex")
});

static RE_CI3_HELPER_SINGLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->helper\s*\(\s*['"]([^'"]+)['"]"#).expect("valid regex")
});

static RE_CI3_HELPER_ARRAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$this->load->helper\s*\(\s*(?:array\(|\[)([^)\]]+)(?:\)|\])"#)
        .expect("valid regex")
});

static RE_CI4_USE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"use\s+(App\\[^;]+);"#).expect("valid regex")
});

static RE_QUOTED_ITEM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"['"]([^'"]+)['"]"#).expect("valid regex")
});

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DependencyGraphRequest {
    /// Path root direktori proyek CodeIgniter.
    pub ci_root: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ComponentStats {
    pub controllers: usize,
    pub models: usize,
    pub libraries: usize,
    pub helpers: usize,
    pub views: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ControllerMigrationOrder {
    pub controller: String,
    pub dependencies: Vec<String>,
    pub line_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DependencyGraphResponse {
    pub ci_version: String,
    pub stats: ComponentStats,
    pub most_used_shared_libraries: Vec<String>,
    pub recommended_migration_order: Vec<ControllerMigrationOrder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileCategory {
    Controller,
    Model,
    Library,
    Helper,
    View,
    Other,
}

fn normalize_path_str(p: &str) -> String {
    p.replace('\\', "/")
}

fn normalize_path(path: &Path) -> String {
    normalize_path_str(&path.to_string_lossy())
}

fn categorize_php_file(normalized_path: &str) -> FileCategory {
    let lower = normalized_path.to_lowercase();
    if lower.contains("/controllers/") {
        FileCategory::Controller
    } else if lower.contains("/models/") {
        FileCategory::Model
    } else if lower.contains("/libraries/") {
        FileCategory::Library
    } else if lower.contains("/helpers/") {
        FileCategory::Helper
    } else if lower.contains("/views/") {
        FileCategory::View
    } else {
        FileCategory::Other
    }
}

fn extract_dependencies_from_code(
    content: &str,
    libraries_map: &mut HashMap<String, usize>,
    is_controller: bool,
) -> (Vec<String>, usize) {
    let mut deps_set = BTreeSet::new();

    // CI3 Models
    for cap in RE_CI3_MODEL_SINGLE.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let model_name = m.as_str().trim();
            if !model_name.is_empty() {
                deps_set.insert(model_name.to_string());
            }
        }
    }
    for cap in RE_CI3_MODEL_ARRAY.captures_iter(content) {
        if let Some(arr_inner) = cap.get(1) {
            for item in RE_QUOTED_ITEM.captures_iter(arr_inner.as_str()) {
                if let Some(m) = item.get(1) {
                    let model_name = m.as_str().trim();
                    if !model_name.is_empty() {
                        deps_set.insert(model_name.to_string());
                    }
                }
            }
        }
    }

    // CI3 Libraries
    for cap in RE_CI3_LIBRARY_SINGLE.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let lib_name = m.as_str().trim();
            if !lib_name.is_empty() {
                deps_set.insert(lib_name.to_string());
                *libraries_map.entry(lib_name.to_string()).or_insert(0) += 1;
            }
        }
    }
    for cap in RE_CI3_LIBRARY_ARRAY.captures_iter(content) {
        if let Some(arr_inner) = cap.get(1) {
            for item in RE_QUOTED_ITEM.captures_iter(arr_inner.as_str()) {
                if let Some(m) = item.get(1) {
                    let lib_name = m.as_str().trim();
                    if !lib_name.is_empty() {
                        deps_set.insert(lib_name.to_string());
                        *libraries_map.entry(lib_name.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // CI3 Helpers
    for cap in RE_CI3_HELPER_SINGLE.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let helper_name = m.as_str().trim();
            if !helper_name.is_empty() {
                deps_set.insert(helper_name.to_string());
                *libraries_map.entry(helper_name.to_string()).or_insert(0) += 1;
            }
        }
    }
    for cap in RE_CI3_HELPER_ARRAY.captures_iter(content) {
        if let Some(arr_inner) = cap.get(1) {
            for item in RE_QUOTED_ITEM.captures_iter(arr_inner.as_str()) {
                if let Some(m) = item.get(1) {
                    let helper_name = m.as_str().trim();
                    if !helper_name.is_empty() {
                        deps_set.insert(helper_name.to_string());
                        *libraries_map.entry(helper_name.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // CI4 Namespaces / use
    let mut ci4_use_count = 0;
    for cap in RE_CI4_USE.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let fqcn = m.as_str().trim();
            if !fqcn.is_empty() {
                deps_set.insert(fqcn.to_string());
                ci4_use_count += 1;
            }
        }
    }

    if is_controller {
        let deps: Vec<String> = deps_set.into_iter().collect();
        (deps, ci4_use_count)
    } else {
        (Vec::new(), ci4_use_count)
    }
}

pub fn scan_ci_directory(ci_root: &Path) -> Result<DependencyGraphResponse, String> {
    if !ci_root.exists() {
        return Err(format!("Directory '{}' does not exist.", ci_root.display()));
    }
    if !ci_root.is_dir() {
        return Err(format!("Path '{}' is not a directory.", ci_root.display()));
    }

    let mut controller_files = Vec::new();
    let mut model_files = Vec::new();
    let mut library_count = 0usize;
    let mut helper_count = 0usize;
    let mut view_count = 0usize;

    let mut ci3_indicators = 0usize;
    let mut ci4_indicators = 0usize;

    // Check for CI4 spark file or App namespace
    if ci_root.join("spark").exists() {
        ci4_indicators += 5;
    }
    if ci_root.join("system/core/CodeIgniter.php").exists() {
        ci3_indicators += 5;
    }

    let walker = WalkDir::new(ci_root)
        .into_iter()
        .filter_entry(|entry| {
            let file_name = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir() {
                if file_name == "vendor"
                    || file_name == "system"
                    || file_name == "node_modules"
                    || file_name == ".git"
                {
                    return false;
                }
            }
            true
        });

    for entry_res in walker {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let filename_lossy = entry.file_name().to_string_lossy();

        if !filename_lossy.ends_with(".php") {
            continue;
        }

        let normalized = normalize_path(path);

        // Extra safeguard: exclude vendor/system/node_modules/.git if nested
        if normalized.contains("/vendor/")
            || normalized.contains("/system/")
            || normalized.contains("/node_modules/")
            || normalized.contains("/.git/")
        {
            continue;
        }

        match categorize_php_file(&normalized) {
            FileCategory::Controller => {
                controller_files.push(path.to_path_buf());
            }
            FileCategory::Model => {
                model_files.push(path.to_path_buf());
            }
            FileCategory::Library => {
                library_count += 1;
            }
            FileCategory::Helper => {
                helper_count += 1;
            }
            FileCategory::View => {
                view_count += 1;
            }
            FileCategory::Other => {}
        }
    }

    let mut libraries_usage: HashMap<String, usize> = HashMap::new();

    // Process models for dependency tracking and library usage
    for model_path in &model_files {
        if let Ok(content) = fs::read_to_string(model_path) {
            let (_, ci4_uses) = extract_dependencies_from_code(&content, &mut libraries_usage, false);
            ci4_indicators += ci4_uses;
            if content.contains("$this->load->") {
                ci3_indicators += 1;
            }
        }
    }

    let mut controller_orders: Vec<ControllerMigrationOrder> = Vec::new();

    // Process controllers
    for controller_path in &controller_files {
        let relative_path = match controller_path.strip_prefix(ci_root) {
            Ok(rel) => normalize_path(rel),
            Err(_) => normalize_path(controller_path),
        };

        let content = fs::read_to_string(controller_path).unwrap_or_default();
        let line_count = content.lines().count();

        if content.contains("$this->load->") {
            ci3_indicators += 1;
        }

        let (dependencies, ci4_uses) =
            extract_dependencies_from_code(&content, &mut libraries_usage, true);
        ci4_indicators += ci4_uses;

        controller_orders.push(ControllerMigrationOrder {
            controller: relative_path,
            dependencies,
            line_count,
        });
    }

    // Sort migration order by complexity: (total_dependencies + line_count) ascending
    controller_orders.sort_by(|a, b| {
        let score_a = a.dependencies.len() + a.line_count;
        let score_b = b.dependencies.len() + b.line_count;
        score_a
            .cmp(&score_b)
            .then_with(|| a.controller.cmp(&b.controller))
    });

    // Determine CI version
    let ci_version = if ci4_indicators > ci3_indicators {
        "CodeIgniter 4".to_string()
    } else {
        "CodeIgniter 3".to_string()
    };

    // Sort most used libraries by frequency desc, then name asc
    let mut lib_vec: Vec<(String, usize)> = libraries_usage.into_iter().collect();
    lib_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let most_used_shared_libraries: Vec<String> = lib_vec.into_iter().take(10).map(|(k, _)| k).collect();

    Ok(DependencyGraphResponse {
        ci_version,
        stats: ComponentStats {
            controllers: controller_files.len(),
            models: model_files.len(),
            libraries: library_count,
            helpers: helper_count,
            views: view_count,
        },
        most_used_shared_libraries,
        recommended_migration_order: controller_orders,
    })
}

pub async fn handle_build_dependency_graph(req: DependencyGraphRequest) -> CallToolResult {
    let ci_root_path = PathBuf::from(req.ci_root.trim());
    match scan_ci_directory(&ci_root_path) {
        Ok(response) => match serde_json::to_value(&response) {
            Ok(json_val) => CallToolResult::structured(json_val),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to serialize response: {}",
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
    fn test_scan_ci3_project() {
        let dir = tempdir().expect("create temp dir");
        let root = dir.path();

        // Create directory structure
        let app_dir = root.join("application");
        let controllers_dir = app_dir.join("controllers");
        let models_dir = app_dir.join("models");
        let libraries_dir = app_dir.join("libraries");
        let helpers_dir = app_dir.join("helpers");
        let views_dir = app_dir.join("views");
        let vendor_dir = root.join("vendor").join("dummy");

        fs::create_dir_all(&controllers_dir).unwrap();
        fs::create_dir_all(&models_dir).unwrap();
        fs::create_dir_all(&libraries_dir).unwrap();
        fs::create_dir_all(&helpers_dir).unwrap();
        fs::create_dir_all(&views_dir).unwrap();
        fs::create_dir_all(&vendor_dir).unwrap();

        // Controller 1: Simple Auth
        let mut f1 = File::create(controllers_dir.join("Auth.php")).unwrap();
        writeln!(
            f1,
            r#"<?php
defined('BASEPATH') OR exit('No direct script access allowed');

class Auth extends CI_Controller {{
    public function __construct() {{
        parent::__construct();
        $this->load->model('User_model');
        $this->load->library('ion_auth');
    }}

    public function login() {{
        $this->load->view('auth/login');
    }}
}}
"#
        )
        .unwrap();

        // Controller 2: Complex Orders with more dependencies & lines
        let mut f2 = File::create(controllers_dir.join("Orders.php")).unwrap();
        writeln!(
            f2,
            r#"<?php
class Orders extends CI_Controller {{
    public function __construct() {{
        parent::__construct();
        $this->load->model('Order_model');
        $this->load->model('User_model');
        $this->load->library('ion_auth');
        $this->load->library('pdf_generator');
        $this->load->helper('url');
    }}
    public function index() {{}}
    public function create() {{}}
    public function edit() {{}}
    public function delete() {{}}
}}
"#
        )
        .unwrap();

        // Model
        let mut f3 = File::create(models_dir.join("User_model.php")).unwrap();
        writeln!(
            f3,
            r#"<?php
class User_model extends CI_Model {{
    public function __construct() {{
        parent::__construct();
        $this->load->library('ion_auth');
    }}
}}
"#
        )
        .unwrap();

        // Library & Helper & View
        File::create(libraries_dir.join("Pdf_generator.php")).unwrap();
        File::create(helpers_dir.join("custom_helper.php")).unwrap();
        File::create(views_dir.join("login.php")).unwrap();

        // Ignored file in vendor
        File::create(vendor_dir.join("IgnoredController.php")).unwrap();

        let result = scan_ci_directory(root).expect("scan must succeed");

        assert_eq!(result.ci_version, "CodeIgniter 3");
        assert_eq!(result.stats.controllers, 2);
        assert_eq!(result.stats.models, 1);
        assert_eq!(result.stats.libraries, 1);
        assert_eq!(result.stats.helpers, 1);
        assert_eq!(result.stats.views, 1);

        // ion_auth is loaded in Auth controller, Orders controller, and User_model (3 times)
        assert!(result.most_used_shared_libraries.contains(&"ion_auth".to_string()));

        // Check recommended migration order: Auth has fewer deps/lines than Orders
        assert_eq!(result.recommended_migration_order.len(), 2);
        assert_eq!(result.recommended_migration_order[0].controller, "application/controllers/Auth.php");
        assert!(result.recommended_migration_order[0].dependencies.contains(&"User_model".to_string()));
        assert!(result.recommended_migration_order[0].dependencies.contains(&"ion_auth".to_string()));
        assert_eq!(result.recommended_migration_order[1].controller, "application/controllers/Orders.php");
    }

    #[test]
    fn test_non_existent_directory() {
        let path = Path::new("/path/that/does/not/exist/ever_12345");
        let result = scan_ci_directory(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }
}
