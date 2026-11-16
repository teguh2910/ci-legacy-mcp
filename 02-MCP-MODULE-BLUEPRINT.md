# Blueprint 02: Tool `extract_module_blueprint`

## 1. Tujuan Tool
Mengompres konteks satu modul CodeIgniter (Controller + Model terkait + daftar View) menjadi satu blueprint JSON terstruktur. Tool ini mencegah *context window overflow* pada LLM dengan menyaring hanya metadata teknis yang dibutuhkan untuk menulis ulang kode ke Laravel.

## 2. Parameter Input (`ModuleBlueprintRequest`)
- `ci_root` (`String`): Path root direktori proyek CodeIgniter.
- `controller_relative_path` (`String`): Path relatif file Controller target (contoh: `application/controllers/Inventory.php`).
- `include_raw_code` (`Option<bool>`): Default `true` jika ukuran total file di bawah 800 baris, atau `false` jika hanya ingin ringkasan struktural.

## 3. Logika Ekstraksi di Rust (`src/tools/module_blueprint.rs`)
1. Baca file Controller target dan ekstrak menggunakan Regex:
   - **Public Methods (Endpoints):** `public\s+function\s+([a-zA-Z0-9_]+)\s*\(([^)]*)\)` (abaikan `__construct`).
   - **HTTP Inputs:** Semua kemunculan `\$this->input->(post|get|get_post)\s*\(\s*['"]([^'"]+)['"]` serta akses superglobal `\$_(POST|GET|FILES)\[['"]([^'"]+)['"]\]`.
   - **Session & Flashdata:** Semua kemunculan `\$this->session->(userdata|set_userdata|flashdata|set_flashdata)\s*\(\s*['"]([^'"]+)['"]`.
   - **Views Rendered:** Semua kemunculan `\$this->load->view\s*\(\s*['"]([^'"]+)['"]` (CI3) atau `return\s+view\s*\(\s*['"]([^'"]+)['"]` (CI4).
   - **Loaded Models:** Semua model yang di-load di dalam Controller ini.
2. Temukan file Model yang terhubung dari langkah di atas, baca isinya, dan ekstrak:
   - Daftar nama fungsi di Model.
   - Semua referensi tabel database pada Query Builder (`->get\(['"]([^'"]+)['"]`, `->from\(['"]([^'"]+)['"]`, `->join\(['"]([^'"]+)['"]`, `->insert\(['"]([^'"]+)['"]`, `->update\(['"]([^'"]+)['"]`) serta deteksi apakah ada raw SQL (`->query\(`).
3. Verifikasi keberadaan file View yang dirender di folder `views/` dan sertakan path-nya.

## 4. Format Output JSON ke LLM
Kembalikan JSON berisi:
- `controller_path` & `endpoints` (nama method + parameter)
- `inputs_detected` (POST/GET/FILES)
- `session_keys`
- `linked_models` (beserta daftar method & tabel yang diakses)
- `linked_views` (path file view yang ditemukan)
- `raw_sources` (isi kode Controller & Model jika `include_raw_code` aktif).