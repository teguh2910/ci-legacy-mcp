use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use once_cell::sync::Lazy;
use regex::Regex;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

static RE_CREATE_TABLE_HEADER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?['"`]?([a-zA-Z0-9_]+)['"`]?\s*\("#)
        .expect("valid regex")
});

static RE_ALL_TABLES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?['"`]?([a-zA-Z0-9_]+)['"`]?"#)
        .expect("valid regex")
});

static RE_PRIMARY_KEY_TABLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)PRIMARY\s+KEY\s*\(([^)]+)\)"#).expect("valid regex")
});

static RE_FOREIGN_KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)(?:CONSTRAINT\s+['"`]?[a-zA-Z0-9_]+['"`]?\s+)?FOREIGN\s+KEY\s*\(([^)]+)\)\s+REFERENCES\s+['"`]?([a-zA-Z0-9_]+)['"`]?\s*\(([^)]+)\)"#)
        .expect("valid regex")
});

static RE_DEFAULT_VAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)DEFAULT\s+('([^']*)'|"([^"]*)"|([^\s,;]+))"#).expect("valid regex")
});

static RE_COLUMN_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^['"`]?([a-zA-Z0-9_]+)['"`]?\s+([a-zA-Z0-9_]+(?:\([^)]+\))?(?:\s+unsigned)?)(.*)$"#)
        .expect("valid regex")
});

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DbSchemaRequest {
    /// Path menuju file .sql struktur database proyek legacy.
    pub sql_dump_path: String,
    /// Daftar nama tabel yang ingin diekstrak strukturnya (jika kosong, kembalikan daftar seluruh nama tabel).
    #[serde(default)]
    pub table_names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ColumnDefinition {
    pub column_name: String,
    pub raw_sql_type: String,
    pub is_nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub laravel_migration_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<String>,
    pub raw_ddl: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AllTablesResponse {
    pub total_tables: usize,
    pub tables: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TablesSchemaResponse {
    pub tables: Vec<TableSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_found_tables: Vec<String>,
}

fn clean_identifier(s: &str) -> String {
    s.trim().trim_matches(|c| c == '`' || c == '\'' || c == '"').to_string()
}

fn split_ddl_items(inner_ddl: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut quote_char = ' ';

    for ch in inner_ddl.chars() {
        match ch {
            '\'' | '"' | '`' => {
                if in_quote && ch == quote_char {
                    in_quote = false;
                } else if !in_quote {
                    in_quote = true;
                    quote_char = ch;
                }
                current.push(ch);
            }
            '(' if !in_quote => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_quote => {
                if depth > 0 {
                    depth -= 1;
                }
                current.push(ch);
            }
            ',' if !in_quote && depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    items.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        items.push(trimmed.to_string());
    }
    items
}

fn extract_table_blocks(sql: &str) -> HashMap<String, (String, String, String)> {
    let mut map = HashMap::new();

    for mat in RE_CREATE_TABLE_HEADER.find_iter(sql) {
        let header = mat.as_str();
        let cap = match RE_CREATE_TABLE_HEADER.captures(header) {
            Some(c) => c,
            None => continue,
        };
        let table_name = clean_identifier(cap.get(1).unwrap().as_str());

        let start_pos = mat.end();
        let mut depth = 1usize;
        let mut in_quote = false;
        let mut quote_char = ' ';
        let mut end_pos = None;

        for (idx, ch) in sql[start_pos..].char_indices() {
            match ch {
                '\'' | '"' | '`' => {
                    if in_quote && ch == quote_char {
                        in_quote = false;
                    } else if !in_quote {
                        in_quote = true;
                        quote_char = ch;
                    }
                }
                '(' if !in_quote => {
                    depth += 1;
                }
                ')' if !in_quote => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(start_pos + idx);
                        break;
                    }
                }
                _ => {}
            }
        }

        if let Some(inner_end) = end_pos {
            let inner_ddl = &sql[start_pos..inner_end];
            let mut full_end = inner_end + 1;
            while full_end < sql.len() && &sql[full_end..full_end + 1] != ";" {
                full_end += 1;
            }
            if full_end < sql.len() && &sql[full_end..full_end + 1] == ";" {
                full_end += 1;
            }
            let full_ddl = &sql[mat.start()..full_end];
            map.insert(
                table_name.to_lowercase(),
                (table_name, inner_ddl.to_string(), full_ddl.trim().to_string()),
            );
        }
    }

    map
}

fn map_to_laravel_migration(raw_type: &str, is_auto_inc: bool) -> String {
    let lower = raw_type.to_lowercase();
    if is_auto_inc {
        if lower.starts_with("bigint") {
            return "$table->id()".to_string();
        }
        return "$table->increments()".to_string();
    }

    if lower.starts_with("bigint") {
        return "$table->bigInteger()".to_string();
    }
    if lower.starts_with("tinyint(1)") || lower == "bool" || lower == "boolean" {
        return "$table->boolean()".to_string();
    }
    if lower.starts_with("tinyint") {
        return "$table->tinyInteger()".to_string();
    }
    if lower.starts_with("smallint") {
        return "$table->smallInteger()".to_string();
    }
    if lower.starts_with("mediumint") {
        return "$table->mediumInteger()".to_string();
    }
    if lower.starts_with("int") || lower.starts_with("integer") {
        return "$table->integer()".to_string();
    }
    if lower.starts_with("varchar") || lower.starts_with("char") {
        return "$table->string()".to_string();
    }
    if lower.starts_with("longtext") {
        return "$table->longText()".to_string();
    }
    if lower.starts_with("mediumtext") {
        return "$table->mediumText()".to_string();
    }
    if lower.starts_with("text") || lower.starts_with("tinytext") {
        return "$table->text()".to_string();
    }
    if lower.starts_with("decimal") || lower.starts_with("numeric") {
        return "$table->decimal()".to_string();
    }
    if lower.starts_with("double") {
        return "$table->double()".to_string();
    }
    if lower.starts_with("float") {
        return "$table->float()".to_string();
    }
    if lower.starts_with("datetime") {
        return "$table->dateTime()".to_string();
    }
    if lower.starts_with("timestamp") {
        return "$table->timestamp()".to_string();
    }
    if lower.starts_with("date") {
        return "$table->date()".to_string();
    }
    if lower.starts_with("time") {
        return "$table->time()".to_string();
    }
    if lower.starts_with("json") {
        return "$table->json()".to_string();
    }
    if lower.starts_with("enum") {
        return "$table->enum()".to_string();
    }

    "$table->string()".to_string()
}

pub fn parse_sql_dump(
    sql_path: &Path,
    requested_tables: &[String],
) -> Result<serde_json::Value, String> {
    if !sql_path.exists() {
        return Err(format!("SQL dump file '{}' does not exist.", sql_path.display()));
    }
    if !sql_path.is_file() {
        return Err(format!("Path '{}' is not a file.", sql_path.display()));
    }

    let content = fs::read_to_string(sql_path)
        .map_err(|e| format!("Failed to read SQL dump file '{}': {}", sql_path.display(), e))?;

    // If requested_tables is empty, extract and return all table names
    if requested_tables.is_empty() {
        let mut tables = Vec::new();
        for cap in RE_ALL_TABLES.captures_iter(&content) {
            if let Some(m) = cap.get(1) {
                let tbl = clean_identifier(m.as_str());
                if !tables.contains(&tbl) {
                    tables.push(tbl);
                }
            }
        }
        let total_tables = tables.len();
        return serde_json::to_value(&AllTablesResponse { total_tables, tables })
            .map_err(|e| format!("JSON serialization error: {}", e));
    }

    let table_blocks = extract_table_blocks(&content);

    let mut parsed_tables = Vec::new();
    let mut not_found_tables = Vec::new();

    for target_table in requested_tables {
        let clean_target = clean_identifier(target_table);
        let key = clean_target.to_lowercase();

        if let Some((orig_name, inner_content, full_ddl)) = table_blocks.get(&key) {
            let ddl_items = split_ddl_items(inner_content);

            let mut table_primary_keys = Vec::new();
            let mut foreign_keys = Vec::new();

            // First pass: identify primary keys and foreign keys from constraints
            for item in &ddl_items {
                let trimmed = item.trim();
                let upper = trimmed.to_uppercase();

                if upper.starts_with("PRIMARY KEY") {
                    if let Some(pk_cap) = RE_PRIMARY_KEY_TABLE.captures(trimmed) {
                        if let Some(pk_inner) = pk_cap.get(1) {
                            for col in pk_inner.as_str().split(',') {
                                table_primary_keys.push(clean_identifier(col));
                            }
                        }
                    }
                } else if upper.contains("FOREIGN KEY") {
                    if let Some(fk_cap) = RE_FOREIGN_KEY.captures(trimmed) {
                        let fk_cols = fk_cap.get(1).map(|m| clean_identifier(m.as_str())).unwrap_or_default();
                        let ref_table = fk_cap.get(2).map(|m| clean_identifier(m.as_str())).unwrap_or_default();
                        let ref_cols = fk_cap.get(3).map(|m| clean_identifier(m.as_str())).unwrap_or_default();
                        foreign_keys.push(format!("{} -> {}({})", fk_cols, ref_table, ref_cols));
                    } else {
                        foreign_keys.push(trimmed.to_string());
                    }
                }
            }

            // Second pass: parse columns
            let mut columns = Vec::new();

            for item in &ddl_items {
                let trimmed = item.trim();
                let upper = trimmed.to_uppercase();

                // Skip constraint/key statements
                if upper.starts_with("PRIMARY KEY")
                    || upper.starts_with("KEY")
                    || upper.starts_with("INDEX")
                    || upper.starts_with("UNIQUE KEY")
                    || upper.starts_with("FULLTEXT")
                    || upper.starts_with("CONSTRAINT")
                    || upper.starts_with("FOREIGN KEY")
                {
                    continue;
                }

                if let Some(col_cap) = RE_COLUMN_LINE.captures(trimmed) {
                    let col_name = clean_identifier(col_cap.get(1).unwrap().as_str());
                    let raw_type = col_cap.get(2).unwrap().as_str().trim().to_string();
                    let modifiers = col_cap.get(3).map(|m| m.as_str().trim()).unwrap_or("");
                    let mod_upper = modifiers.to_uppercase();

                    let is_auto_inc = mod_upper.contains("AUTO_INCREMENT");
                    let is_nullable = !mod_upper.contains("NOT NULL");

                    let mut default_value = None;
                    if let Some(def_cap) = RE_DEFAULT_VAL.captures(modifiers) {
                        if let Some(val) = def_cap.get(2).or_else(|| def_cap.get(3)).or_else(|| def_cap.get(4)) {
                            default_value = Some(val.as_str().to_string());
                        }
                    }

                    let is_pk = mod_upper.contains("PRIMARY KEY")
                        || table_primary_keys.contains(&col_name);

                    if is_pk && !table_primary_keys.contains(&col_name) {
                        table_primary_keys.push(col_name.clone());
                    }

                    let laravel_migration_type = map_to_laravel_migration(&raw_type, is_auto_inc);

                    columns.push(ColumnDefinition {
                        column_name: col_name,
                        raw_sql_type: raw_type,
                        is_nullable,
                        default_value,
                        is_primary_key: is_pk,
                        is_auto_increment: is_auto_inc,
                        laravel_migration_type,
                    });
                }
            }

            parsed_tables.push(TableSchema {
                table_name: orig_name.clone(),
                columns,
                primary_keys: table_primary_keys,
                foreign_keys,
                raw_ddl: full_ddl.clone(),
            });
        } else {
            not_found_tables.push(clean_target);
        }
    }

    serde_json::to_value(&TablesSchemaResponse {
        tables: parsed_tables,
        not_found_tables,
    })
    .map_err(|e| format!("JSON serialization error: {}", e))
}

pub async fn handle_inspect_database_schema(req: DbSchemaRequest) -> CallToolResult {
    let sql_path = PathBuf::from(req.sql_dump_path.trim());
    match parse_sql_dump(&sql_path, &req.table_names) {
        Ok(json_val) => CallToolResult::structured(json_val),
        Err(err_msg) => CallToolResult::error(vec![ContentBlock::text(err_msg)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    const SAMPLE_SQL: &str = r#"
CREATE TABLE `users` (
  `id` int(11) NOT NULL AUTO_INCREMENT,
  `username` varchar(100) NOT NULL,
  `email` varchar(255) NOT NULL,
  `password_hash` varchar(255) NOT NULL,
  `bio` text DEFAULT NULL,
  `is_active` tinyint(1) NOT NULL DEFAULT '1',
  `created_at` datetime NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS `orders` (
  `order_id` bigint(20) NOT NULL AUTO_INCREMENT,
  `user_id` int(11) NOT NULL,
  `total_amount` decimal(15,2) NOT NULL DEFAULT '0.00',
  `status` enum('pending','paid','shipped') NOT NULL DEFAULT 'pending',
  `notes` mediumtext,
  PRIMARY KEY (`order_id`),
  CONSTRAINT `fk_orders_user` FOREIGN KEY (`user_id`) REFERENCES `users` (`id`) ON DELETE CASCADE
);
"#;

    #[test]
    fn test_parse_sql_list_all_tables() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("schema.sql");
        let mut f = File::create(&file_path).unwrap();
        f.write_all(SAMPLE_SQL.as_bytes()).unwrap();

        let val = parse_sql_dump(&file_path, &[]).unwrap();
        let resp: AllTablesResponse = serde_json::from_value(val).unwrap();

        assert_eq!(resp.total_tables, 2);
        assert_eq!(resp.tables, vec!["users", "orders"]);
    }

    #[test]
    fn test_parse_sql_specific_tables() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("schema.sql");
        let mut f = File::create(&file_path).unwrap();
        f.write_all(SAMPLE_SQL.as_bytes()).unwrap();

        let val = parse_sql_dump(&file_path, &["users".to_string(), "orders".to_string()]).unwrap();
        let resp: TablesSchemaResponse = serde_json::from_value(val).unwrap();

        assert_eq!(resp.tables.len(), 2);
        assert!(resp.not_found_tables.is_empty());

        let users_tbl = &resp.tables[0];
        assert_eq!(users_tbl.table_name, "users");
        assert_eq!(users_tbl.primary_keys, vec!["id"]);
        assert_eq!(users_tbl.columns.len(), 7);

        let id_col = &users_tbl.columns[0];
        assert_eq!(id_col.column_name, "id");
        assert!(id_col.is_auto_increment);
        assert!(id_col.is_primary_key);
        assert_eq!(id_col.laravel_migration_type, "$table->increments()");

        let is_active_col = users_tbl.columns.iter().find(|c| c.column_name == "is_active").unwrap();
        assert_eq!(is_active_col.laravel_migration_type, "$table->boolean()");
        assert_eq!(is_active_col.default_value.as_deref(), Some("1"));

        let orders_tbl = &resp.tables[1];
        assert_eq!(orders_tbl.table_name, "orders");
        let order_id = &orders_tbl.columns[0];
        assert_eq!(order_id.laravel_migration_type, "$table->id()");

        let amount_col = orders_tbl.columns.iter().find(|c| c.column_name == "total_amount").unwrap();
        assert_eq!(amount_col.laravel_migration_type, "$table->decimal()");

        assert_eq!(orders_tbl.foreign_keys.len(), 1);
        assert!(orders_tbl.foreign_keys[0].contains("user_id -> users(id)"));
    }

    #[test]
    fn test_parse_sql_missing_file() {
        let path = Path::new("/non_existent/schema.sql");
        let res = parse_sql_dump(path, &[]);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist"));
    }
}
