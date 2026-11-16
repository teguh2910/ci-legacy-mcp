# Blueprint 04: Tool `track_migration_state`

## 1. Tujuan Tool
Menjaga konsistensi lintas sesi chat pada migrasi ratusan modul dengan menyimpan state progres dan kamus pemetaan kelas (*class mapping registry*) ke dalam file `.migration-state.json` di root workspace.

## 2. Parameter Input (`TrackerRequest`)
- `workspace_root` (`String`): Path root workspace proyek.
- `action` (`String`): Salah satu dari `"get_state"`, `"update_module"`, atau `"register_mapping"`.
- `module_name` (`Option<String>`): Nama modul/controller (wajib untuk `update_module`).
- `status` (`Option<String>`): Status baru: `"PENDING"`, `"IN_PROGRESS"`, `"DONE"`, atau `"NEEDS_REVIEW"`.
- `notes` (`Option<String>`): Catatan teknis atau daftar file Laravel yang dihasilkan.
- `legacy_name` (`Option<String>`): Nama class/file lama di CI (misal: `M_barang` atau `ion_auth`).
- `laravel_fqcn` (`Option<String>`): Nama class baru di Laravel (misal: `App\Models\Product` atau `App\Services\AuthService`).

## 3. Logika Penyimpanan State (`src/tools/tracker.rs` & `src/state.rs`)
1. Lokasi file state: `<workspace_root>/.migration-state.json`. Jika belum ada, inisialisasi struktur default.
2. Struktur data JSON:
   ```json
   {
     "last_updated": "2026-11-16T21:00:00Z",
     "class_mappings": {
       "M_barang": "App\\Models\\Product",
       "Auth_model": "App\\Models\\User"
     },
     "modules": {
       "application/controllers/Auth.php": {
         "status": "DONE",
         "generated_files": [
           "app/Http/Controllers/AuthController.php",
           "app/Http/Requests/LoginRequest.php"
         ],
         "notes": "Session CI diganti ke Laravel Sanctum/Session guard"
       }
     }
   }
   ```
3. Pada aksi `"get_state"`, kembalikan seluruh isi `.migration-state.json` beserta ringkasan jumlah modul per status (`DONE`, `IN_PROGRESS`, `PENDING`) agar LLM selalu tahu class Laravel apa saja yang sudah dibuat sebelumnya dan tidak membuat duplikat model/service dengan nama berbeda.