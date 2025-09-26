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
- [ ] Logic trong `aether-cli` có thể xác định thư mục hiện tại là một dự án NodeJS bằng cách kiểm tra sự tồn tại của file `package.json`.
- [ ] Nếu không phải dự án NodeJS, `aether deploy` sẽ báo lỗi và thoát.
- [ ] CLI có khả năng thực thi một tiến trình con (child process) để chạy lệnh `npm install --production`.
- [ ] Output (stdout/stderr) từ lệnh `npm` được hiển thị trực tiếp cho người dùng.
- [ ] Nếu `npm install` thất bại, `aether deploy` sẽ báo lỗi và dừng lại.
- [ ] Sau khi cài đặt dependencies thành công, CLI sẽ nén toàn bộ nội dung của thư mục dự án (bao gồm `node_modules`) thành một file `app.tar.gz`.
- [ ] Các file không cần thiết như `.git`, `target`, `.DS_Store` cần được loại trừ khỏi file nén.
- [ ] Sau khi nén thành công, CLI sẽ in ra đường dẫn của file `app.tar.gz` và kích thước của nó.

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
- **Unit Tests:**
  - [ ] Viết test cho hàm phát hiện `package.json`.
  - [ ] Viết test cho logic lọc các file/thư mục không cần thiết.
- **Integration Tests:**
  - [ ] Tạo một dự án NodeJS mẫu trong thư mục `tests/fixtures/sample-nodejs-app`.
  - [ ] Viết một bài test tích hợp cho `aether deploy`:
    1.  `cd` vào thư mục dự án mẫu.
    2.  Chạy lệnh `aether deploy` (có thể cần thêm một flag như `--dry-run` hoặc `--pack-only` để nó chỉ thực hiện đóng gói mà không upload).
    3.  Xác minh rằng file `app.tar.gz` được tạo ra.
    4.  Giải nén file `app.tar.gz` trong một thư mục tạm và xác minh nội dung của nó là chính xác (có `node_modules`, `package.json`, không có `.git`).
  - [ ] Viết một bài test cho trường hợp `deploy` trong một thư mục không có `package.json` và xác minh CLI báo lỗi.
  - [ ] Viết một bài test cho dự án NodeJS có `npm install` bị lỗi (ví dụ: `package.json` sai cú pháp) và xác minh CLI báo lỗi.
- **Kiểm thử Thủ công:**
  - [ ] Tạo một dự án NodeJS đơn giản.
  - [ ] Chạy `aether deploy` và kiểm tra xem `app.tar.gz` có được tạo ra không.
  - [ ] Kiểm tra nội dung file nén.
