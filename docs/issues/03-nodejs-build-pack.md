# Issue #3: Triển khai logic Build & Đóng gói cho NodeJS

**Tên Issue:** 🚀 [FEAT] - CLI: Logic phát hiện, build và đóng gói ứng dụng NodeJS

**Nhãn:** `enhancement`, `cli`, `nodejs`

**Người thực hiện:** (Để trống)

---

### 1. Mô tả (Description)
Đây là một trong những issue cốt lõi của AetherEngine. Chúng ta sẽ mở rộng `aether-cli` để nó có khả năng:
1.  Tự động phát hiện một thư mục có chứa dự án NodeJS.
2.  Thực thi các lệnh build của NodeJS (`npm install --production`).
3.  Nén toàn bộ mã nguồn và các dependencies đã cài đặt thành một file artifact (`.tar.gz`).

Logic này sẽ được kích hoạt bởi lệnh `aether deploy`.

### 2. Tiêu chí Hoàn thành (Definition of Done)
- [x] Logic trong `aether-cli` xác định dự án NodeJS bằng `package.json`.
- [x] Nếu không phải dự án NodeJS, `aether deploy` trả về lỗi (exit code 2 – usage).
- [x] CLI có thể thực thi tiến trình con chạy `npm install --production` (khi không dùng `--pack-only`).
- [x] Output của `npm` hiển thị trực tiếp (sử dụng kế thừa stdio của `Command`).
- [x] Nếu `npm install` thất bại hoặc thiếu `npm`, CLI trả lỗi Runtime (exit code 20).
- [x] Sau khi cài đặt (hoặc bỏ qua với `--pack-only`), CLI nén nội dung vào file `app-<sha256>.tar.gz`.
- [x] Loại trừ `.git`, `target`, `node_modules` (tránh repackage artifact cũ), `.DS_Store` và các mẫu trong `.aetherignore`.
- [x] In ra đường dẫn & kích thước artifact sau khi tạo.

### 3. Thiết kế & Kiến trúc (Design & Architecture)
- **Phát hiện dự án:**
  - Sử dụng `std::fs::metadata("package.json").is_ok()` để kiểm tra.
- **Thực thi lệnh:**
  - Sử dụng `std::process::Command` để chạy `npm`.
  - Cấu hình `Command` để kế thừa `stdout` và `stderr` giúp người dùng thấy được tiến trình.
  - Kiểm tra `status.success()` để xác định lệnh có thành công hay không.
- **Nén Artifact:**
  - Sử dụng các crate như `tar` và `flate2` (cho gzip) để tạo file `.tar.gz`.
  - Cần có logic để duyệt cây thư mục và thêm từng file/thư mục vào bộ lưu trữ tar.
  - Implement một danh sách các file/thư mục cần loại trừ (ignore list).

  ```rust
  // Ví dụ logic nén
  use flate2::write::GzEncoder;
  use flate2::Compression;
  use std::fs::File;
  use tar::Builder;

  fn create_artifact(path: &str, output_file: &str) -> Result<(), std::io::Error> {
      let file = File::create(output_file)?;
      let enc = GzEncoder::new(file, Compression::default());
      let mut tar_builder = Builder::new(enc);

      // Thêm thư mục vào tar, có thể dùng walkdir để duyệt và lọc
      tar_builder.append_dir_all(".", path)?;

      tar_builder.finish()?;
      Ok(())
  }
  ```

### 4. Yêu cầu về Kiểm thử (Testing Requirements)
- **Unit / Integration (đã thực thi dạng integration):**
  - [x] Test tạo artifact và tôn trọng `.aetherignore` (`deploy_artifact.rs`).
  - [x] Test dự án không phải NodeJS trả về lỗi usage (`deploy_non_node.rs`).
  - [x] Test chế độ chỉ đóng gói (`--pack-only`) tạo artifact (`deploy_non_node.rs`).
  - [x] Test dry-run (đã tồn tại trong `cli_basic.rs`).
  - [x] Test lỗi `npm install` (tạo `package.json` hỏng -> kiểm tra exit code runtime / failure path `deploy_npm_fail.rs`).
  - [ ] Test explicit detection helper ở mức unit (chưa thực hiện – logic hiện đang internal; có thể tách public rồi bổ sung sau).
- **Kiểm thử Thủ công (pending/manual):**
  - [ ] Chạy `npm init -y && aether deploy` để xác thực install thật nếu môi trường có Node.
  - [ ] Giải nén artifact và xác minh `node_modules` hiện diện khi không dùng `--pack-only`.

### 5. Ghi chú Hiện Trạng (Cập nhật)
- Phần cốt lõi ban đầu: HOÀN THÀNH.
- ĐÃ TRIỂN KHAI thêm tất cả hạng mục mở rộng (trừ upload thật sự):
  * Prune devDependencies (`npm prune --production`).
  * Phát hiện package manager: pnpm > yarn > npm (dựa trên lockfile & binary tồn tại).
  * Cache `node_modules` theo hash lockfile + `NODE_VERSION` (copy restore/save).
  * Streaming hash & tar (đọc chunk 64KB, song song cập nhật băm toàn cục + từng file).
  * Flag `--compression-level` (1–9) điều chỉnh gzip.
  * Hợp nhất `.gitignore` + `.aetherignore` (cảnh báo pattern lỗi).
  * Flag `--out` chọn thư mục / file đích.
  * Sinh manifest JSON `<artifact>.manifest.json` (path, size, sha256, tổng số file, tổng kích thước).
  * Test lỗi npm hỏng (`deploy_npm_fail.rs`).
  * Thêm flags mới: `--no-upload`, `--no-cache`.
- Upload hiện tại: MOCK (ghi log nếu có `AETHER_API_BASE`), CHƯA gọi Control Plane thật.
- Artifact vẫn tên mặc định `app-<sha256>.tar.gz` nếu không chỉ định `--out`.
- Manifest đặt cạnh artifact, ví dụ: `app-<sha>.tar.gz.manifest.json`.
- Đã cập nhật các tests: `deploy_artifact.rs`, `deploy_out_and_manifest.rs`, `deploy_npm_fail.rs`, `deploy_cache.rs`.

### 6. Hạng Mục Mở Rộng (Trạng Thái)
| # | Mục | Trạng thái | Ghi chú |
|---|-----|------------|---------|
| 1 | Prune devDependencies | ✅ | `npm prune --production` sau install (npm) |
| 2 | Yarn / PNPM detection | ✅ | Ưu tiên pnpm > yarn > npm; fallback npm |
| 3 | Cache `node_modules` | ✅ | Key: SHA256(lockfile + NODE_VERSION); copy-based |
| 4 | Streaming hash + tar | ✅ | Chunk 64KB; hashing đồng thời toàn cục & từng file |
| 5 | `--compression-level` | ✅ | Giới hạn 1–9, fallback default nếu invalid |
| 6 | Merge `.gitignore` | ✅ | Cảnh báo pattern lỗi; cộng dồn cùng `.aetherignore` |
| 7 | `--out <path>` | ✅ | Hỗ trợ dir, dir với '/', hoặc file cụ thể |
| 8 | Manifest JSON | ✅ | `<artifact>.manifest.json` (file list + per-file sha256) |
| 9 | Test lỗi npm | ✅ | `deploy_npm_fail.rs` kiểm tra fail path |
| 10 | Upload Control Plane thật | ⏳ | HIỆN TẠI MOCK (log); cần API spec để hoàn thiện |

### 7. TODO & Follow-ups Đề Xuất
1. Tích hợp upload thật (multipart / signed URL) khi Control Plane có endpoint.
2. Chuẩn hoá manifest: thêm overall hash, version schema, format version.
3. SBOM / SPDX hoặc CycloneDX generation (tận dụng manifest hiện có).
4. Tối ưu cache: dùng hardlink / reflink thay vì copy; tùy chọn `--cache-dir` tùy chỉnh.
5. Thêm unit tests riêng cho: `detect_package_manager`, `cache_key`, logic merge ignore.
6. Thêm benchmark (criterion) cho packaging với nhiều file & so sánh các mức nén.
7. Thêm flag `--format json` cho output CLI để script dễ parse (artifact path, digest, manifest path).
8. Thêm lựa chọn `--include-dev` để bỏ qua prune khi cần build-step devDependencies.
9. Cân nhắc loại bỏ dependencies chưa dùng (`base64`, `hex`) hoặc dùng cho hash encoding thống nhất.
10. Thêm chữ ký (signature) cho artifact + manifest (Ed25519) -> chuẩn bị bước supply chain security.
11. Thêm kiểm tra kích thước tối đa artifact & cảnh báo nếu vượt ngưỡng cấu hình.
12. Cải thiện xử lý pattern ignore kiểu directory (tự động append `/` nếu cần).

### 8. Kết Luận (Cập nhật)
Phạm vi mở rộng từ danh sách “Hướng Phát Triển” (1–9) đã hoàn tất đầy đủ. Chỉ còn lại bước upload thực sự (mục 10) và các follow-ups nâng chất bảo mật, hiệu năng, chuẩn hoá.

_Lịch sử_: Bản ban đầu chỉ build + package. Hiện tại lệnh `aether deploy` đã trở thành pipeline mini: detect PM -> (cache restore) -> install -> prune -> package (stream + hash) -> manifest -> (mock upload).

Trạng thái tổng: CORE ✅  | ENHANCEMENTS ✅ (trừ upload thật) | UPLOAD THẬT ⏳
