# Blueprint 03: Tool `inspect_database_schema`

## 1. Tujuan Tool
Membaca struktur tabel database asli dari file SQL dump (`schema.sql`) yang ada di proyek agar LLM dapat membuat file **Laravel Migration** dan properti **Eloquent Model** (`$table`, `$primaryKey`, `$fillable`, `$casts`) dengan tipe data yang 100% akurat tanpa menebak.

## 2. Parameter Input (`DbSchemaRequest`)
- `sql_dump_path` (`String`): Path menuju file `.sql` struktur database proyek legacy.
- `table_names` (`Vec<String>`): Daftar nama tabel yang ingin diekstrak strukturnya (jika kosong, kembalikan daftar seluruh nama tabel yang tersedia di file SQL).

## 3. Logika Parsing di Rust (`src/tools/db_schema.rs`)
1. Baca file `sql_dump_path`.
2. Jika `table_names` kosong:
   - Ekstrak semua nama tabel menggunakan Regex `CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"']?([a-zA-Z0-9_]+)[`"']?` dan kembalikan daftarnya.
3. Jika `table_names` diisi:
   - Ambil blok DDL `CREATE TABLE <nama_tabel> ( ... );` untuk setiap tabel yang diminta.
   - Parse setiap baris definisi kolom untuk mengekstrak:
     - `column_name`
     - `raw_sql_type` (misal: `int(11)`, `varchar(255)`, `decimal(15,2)`, `enum(...)`, `datetime`)
     - `is_nullable` (`NOT NULL` vs `NULL`)
     - `default_value`
     - `is_primary_key` dan `is_auto_increment`
     - Definisi `FOREIGN KEY` jika ada.
   - Sertakan rekomendasi tipe kolom Laravel Migration (misal: `varchar` -> `string`, `int` -> `integer`, `decimal` -> `decimal`, `text` -> `text`).

## 4. Format Output JSON ke LLM
Kembalikan array objek skema per tabel yang diminta lengkap dengan detail kolom, primary key, foreign key, dan raw DDL blok tabel tersebut.