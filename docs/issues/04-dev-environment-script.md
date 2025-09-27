# Issue #4: Script tự động hóa môi trường phát triển

**Tên Issue:** 📜 [CHORE] - Tạo script `dev.sh` để thiết lập môi trường phát triển cục bộ

**Nhãn:** `chore`, `developer-experience`, `infra`

**Người thực hiện:** (Để trống)

---

### 1. Mô tả (Description)
Để đơn giản hóa quá trình thiết lập môi trường phát triển cho các thành viên mới và đảm bảo tính nhất quán, chúng ta cần một script tự động hóa (`dev.sh`). Script này sẽ kiểm tra và cài đặt các công cụ cần thiết như Minikube, Docker, và khởi chạy các dịch vụ phụ thuộc (PostgreSQL, MinIO) dưới dạng container.

Mục tiêu là một lập trình viên chỉ cần chạy một lệnh duy nhất để có một môi trường sẵn sàng cho việc phát triển và kiểm thử AetherEngine.

### 2. Tiêu chí Hoàn thành (Definition of Done)
- [ ] Một file `dev.sh` được tạo ở thư mục gốc của dự án (`appengine/dev.sh`).
- [ ] Script phải có quyền thực thi (`chmod +x dev.sh`).
- [ ] Script có các hàm để kiểm tra xem `docker`, `minikube`, `kubectl` đã được cài đặt hay chưa. Nếu chưa, script sẽ in ra hướng dẫn cài đặt và thoát.
- [ ] Script cung cấp các lệnh con (subcommands) như:
    - `dev.sh start`:
        - Khởi động cluster Minikube nếu nó chưa chạy.
        - Sử dụng Docker để khởi chạy một container PostgreSQL.
        - Sử dụng Docker để khởi chạy một container MinIO.
        - In ra các thông tin cần thiết sau khi khởi động (ví dụ: chuỗi kết nối DB, MinIO endpoint, access/secret keys).
    - `dev.sh stop`:
        - Dừng các container PostgreSQL và MinIO.
        - Tùy chọn: Dừng cluster Minikube (`minikube stop`).
    - `dev.sh status`:
        - Kiểm tra và hiển thị trạng thái của Minikube và các container dịch vụ.
    - `dev.sh help`:
        - In ra hướng dẫn sử dụng script.
- [ ] Script sử dụng các biến môi trường cho các cấu hình (ví dụ: `POSTGRES_PASSWORD`, `MINIO_ROOT_USER`, `MINIO_ROOT_PASSWORD`) và có giá trị mặc định an toàn.
- [ ] Script được viết bằng `bash` và tuân thủ các thực hành tốt nhất (ví dụ: sử dụng `set -euo pipefail`).
- [ ] Có một file `README.md` hoặc một phần trong `DEVELOPMENT.md` giải thích cách sử dụng `dev.sh`.

### 3. Thiết kế & Kiến trúc (Design & Architecture)
- **Cấu trúc Script:** Script nên được chia thành các hàm nhỏ, dễ đọc và có thể tái sử dụng.
  ```bash
  #!/bin/bash
  set -euo pipefail

  # --- Biến và Cấu hình ---
  POSTGRES_DB=${POSTGRES_DB:-aether_dev}
  # ...

  # --- Hàm Helper ---
  check_deps() {
    # ... kiểm tra docker, minikube
  }

  start_postgres() {
    # ... docker run postgres
  }

  start_minio() {
    # ... docker run minio
  }

  # --- Logic chính ---
  main() {
    case "${1-}" in
      start)
        # ...
        ;;
      stop)
        # ...
        ;;
      *)
        # ... in help
        ;;
    esac
  }

  main "$@"
  ```
- **Quản lý Container:** Sử dụng `docker ps -q -f name=<container_name>` để kiểm tra xem container đã chạy chưa. Đặt tên cố định cho các container (`aether-postgres`, `aether-minio`) để dễ quản lý.
- **Persistent Data:** Mount volume cho PostgreSQL và MinIO để dữ liệu không bị mất sau mỗi lần khởi động lại container.

### 4. Yêu cầu về Kiểm thử (Testing Requirements)
- **Kiểm thử Thủ công (Bắt buộc):**
  - [ ] Trên một môi trường sạch (chưa có Minikube hay container), chạy `dev.sh start` và xác minh mọi thứ được thiết lập chính xác.
  - [ ] Chạy `dev.sh status` và kiểm tra output.
  - [ ] Kết nối tới PostgreSQL và MinIO bằng các thông tin được in ra để xác nhận chúng hoạt động.
  - [ ] Chạy `dev.sh stop` và xác minh các dịch vụ đã dừng.
  - [ ] Chạy lại `dev.sh start` để đảm bảo script có thể xử lý trường hợp các tài nguyên đã tồn tại (idempotent).
  - [ ] Test trên các hệ điều hành khác nhau nếu có thể (Linux, macOS).
- **Tự động hóa (Tùy chọn):**
  - [ ] Sử dụng `shellcheck` trong CI/CD pipeline để phân tích và phát hiện các lỗi tiềm ẩn trong script.
