# Blueprint 00: Setup Proyek Rust MCP Server (`ci-legacy-mcp`)

## 1. Tujuan
Membangun binary MCP Server menggunakan bahasa **Rust** dan SDK resmi **`rmcp`** dengan transport **`stdio`** untuk menganalisis, memetakan, dan melacak migrasi proyek CodeIgniter (CI3/CI4) skala besar ke Laravel.

## 2. Struktur Proyek
```text
ci-legacy-mcp/
├── Cargo.toml
└── src/
    ├── main.rs                  # Entry point server rmcp & registrasi semua tools
    ├── state.rs                 # Struct state & penyimpanan .migration-state.json
    └── tools/
        ├── mod.rs
        ├── dependency_graph.rs  # Tool 1: build_dependency_graph
        ├── module_blueprint.rs  # Tool 2: extract_module_blueprint
        ├── db_schema.rs         # Tool 3: inspect_database_schema
        └── tracker.rs           # Tool 4: track_migration_state
```

## 3. Spesifikasi `Cargo.toml`
```toml
[package]
name = "ci-legacy-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
rmcp = { version = "0.1", features = ["server", "transport-io"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
walkdir = "2"
regex = "1"
once_cell = "1"
anyhow = "1"
```

## 4. Aturan Teknis Wajib
1. **Strict `stdio` Safety:** DILARANG keras menggunakan `println!()` di seluruh codebase karena akan merusak aliran JSON-RPC pada `stdout`. Gunakan `eprintln!()` jika butuh logging debugging.
2. **Error Handling:** Setiap error pembacaan file atau parsing tidak boleh membuat server panic (`unwrap()` dihindari untuk input eksternal); kembalikan `CallToolResult::error` dengan pesan yang jelas bagi LLM.
3. **Cross-Platform Path:** Normalisasi pemisah path (`\` menjadi `/`) sebelum melakukan pencocokan string direktori agar berjalan mulus di Linux, macOS, dan Windows.