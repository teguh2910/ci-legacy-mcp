# Blueprint 01: Tool `build_dependency_graph`

## 1. Tujuan Tool
Memindai seluruh repository CodeIgniter legacy untuk memetakan semua modul (Controller, Model, Library, Helper) beserta ketergantungan antar file, sehingga LLM mengetahui urutan migrasi yang paling aman (dari modul independen ke modul kompleks).

## 2. Parameter Input (`DependencyGraphRequest`)
- `ci_root` (`String`): Path root direktori proyek CodeIgniter.

## 3. Logika Pemindaian di Rust (`src/tools/dependency_graph.rs`)
1. Gunakan `WalkDir` untuk menelusuri `ci_root`, abaikan folder `vendor/`, `system/`, `node_modules/`, dan `.git/`.
2. Klasifikasikan setiap file `.php` ke dalam kategori berdasarkan path-nya:
   - `controllers` (berisi `/controllers/` atau `/Controllers/`)
   - `models` (berisi `/models/` atau `/Models/`)
   - `libraries` (berisi `/libraries/` atau `/Libraries/`)
   - `helpers` (berisi `/helpers/` atau `/Helpers/`)
   - `views` (berisi `/views/` atau `/Views/`)
3. Untuk setiap file Controller dan Model, ekstrak dependensi menggunakan Regex:
   - Pemanggilan Model CI3: `\$this->load->model\s*\(\s*['"]([^'"]+)['"]`
   - Pemanggilan Library CI3: `\$this->load->library\s*\(\s*['"]([^'"]+)['"]`
   - Pemanggilan Helper CI3: `\$this->load->helper\s*\(\s*['"]([^'"]+)['"]`
   - Namespace / `use` CI4: `use\s+(App\\[^;]+);`
4. Hitung skor kompleksitas setiap Controller (`total_dependencies` + `line_count`) dan urutkan dari skor terendah ke tertinggi sebagai rekomendasi `migration_order`.

## 4. Format Output JSON ke LLM
Kembalikan JSON ringkas terstruktur:
```json
{
  "ci_version": "CodeIgniter 3",
  "stats": {
    "controllers": 45,
    "models": 38,
    "libraries": 12,
    "helpers": 8,
    "views": 110
  },
  "most_used_shared_libraries": ["ion_auth", "pdf_generator", "excel_helper"],
  "recommended_migration_order": [
    {
      "controller": "application/controllers/Auth.php",
      "dependencies": ["User_model", "ion_auth"],
      "line_count": 140
    }
  ]
}
```