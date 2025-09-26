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
  - [ ] Test lỗi `npm install` (chưa thực hiện do CI có thể thiếu `npm`; sẽ bổ sung với mock hoặc skip có điều kiện).
  - [ ] Test explicit detection helper ở mức unit (có thể tách sau nếu xuất API công khai).
- **Kiểm thử Thủ công (pending/manual):**
  - [ ] Chạy `npm init -y && aether deploy` để xác thực install thật nếu môi trường có Node.
  - [ ] Giải nén artifact và xác minh `node_modules` hiện diện khi không dùng `--pack-only`.

### 5. Ghi chú Hiện Trạng
- Chức năng build & package NodeJS cơ bản: HOÀN THÀNH.
- Thêm cờ `--pack-only` để hỗ trợ CI không có `npm`.
- Artifact đặt tên theo hash nội dung (`app-<sha256>.tar.gz`).
- Tự động loại trừ artifact cũ (`app-*` & `artifact-*`).

### 6. Hướng Phát Triển / Nâng Cấp Tiếp Theo
1. Thêm bước prune devDependencies: `npm prune --production` sau install.
2. Hỗ trợ Yarn / PNPM detection (lockfile ưu tiên: `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`).
3. Cache `node_modules` giữa các lần deploy (hash package-lock + NODE_VERSION làm key).
4. Streaming nén & băm đồng thời để tránh đọc toàn bộ file lớn vào RAM.
5. Thêm cấu hình `compression-level` (gzip level 1-9) qua flag hoặc config.
6. Cho phép exclude mặc định mở rộng: `.gitignore` merge với `.aetherignore`.
7. Thêm flag `--out <path>` chỉ định tên hoặc thư mục artifact.
8. Xuất manifest JSON (liệt kê file + hash) kèm artifact để phục vụ SBOM sau này.
9. Kiểm thử npm lỗi: tạo `package.json` hỏng và assert exit code runtime (skip nếu thiếu npm).
10. Tích hợp upload artifact lên Control Plane (khi API sẵn) thay vì chỉ local.

### 7. Kết Luận
Issue #3 đã HOÀN THÀNH phạm vi cốt lõi. Các mục còn lại được chuyển sang “Hướng Phát Triển”.
