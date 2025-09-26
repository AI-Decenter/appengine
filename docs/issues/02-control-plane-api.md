# Issue #2: Xây dựng Control Plane API cơ bản

**Tên Issue:** 🚀 [FEAT] - Nền tảng Control Plane API với Axum

**Nhãn:** `enhancement`, `control-plane`, `api`, `epic`

**Người thực hiện:** (Để trống)

---

### 1. Mô tả (Description)
Issue này tập trung vào việc xây dựng "bộ não" của AetherEngine - Control Plane. Chúng ta sẽ tạo một dịch vụ web bằng Axum, định nghĩa các API endpoint chính, và thiết lập kết nối ban đầu tới cơ sở dữ liệu PostgreSQL sử dụng `sqlx`.

### 2. Tiêu chí Hoàn thành (Definition of Done)
- [ ] Một crate Rust mới tên là `control-plane` được tạo trong workspace.
- [ ] Các thư viện `axum`, `tokio`, `serde`, `sqlx` (với features `runtime-tokio-rustls`, `postgres`) được thêm vào `Cargo.toml`.
- [ ] Một web server Axum cơ bản có thể khởi động và chạy.
- [ ] API endpoint `GET /health` được tạo, trả về status `200 OK` và một body JSON `{"status": "ok"}`.
- [ ] Thiết lập `sqlx-cli` và tạo migration đầu tiên để định nghĩa bảng `applications` và `deployments`.
- [ ] Cấu hình để server có thể kết nối tới database PostgreSQL thông qua một chuỗi kết nối từ biến môi trường `DATABASE_URL`.
- [ ] Logic khởi tạo state của ứng dụng (ví dụ: database connection pool) và chia sẻ nó với các handler của Axum.
- [ ] Định nghĩa các API endpoint "mock" cho các chức năng cốt lõi:
    - `POST /deployments`: Nhận một body JSON rỗng, trả về `201 Created`.
    - `GET /apps`: Trả về một danh sách JSON rỗng `[]` với status `200 OK`.
    - `GET /apps/{app_name}/logs`: Trả về một chuỗi rỗng với status `200 OK`.
- [ ] Mã nguồn được cấu trúc theo module (ví dụ: `handlers`, `models`, `db`).

### 3. Thiết kế & Kiến trúc (Design & Architecture)
- **Cấu trúc thư mục:**
  ```
  crates/control-plane/
  ├── migrations/
  │   └── <timestamp>_initial_schema.sql
  ├── src/
  │   ├── main.rs       # Khởi tạo server, router, state
  │   ├── handlers/     # Logic xử lý request/response
  │   │   ├── mod.rs
  │   │   ├── health.rs
  │   │   └── apps.rs
  │   ├── models.rs     # Structs (ví dụ: Application, Deployment)
  │   └── db.rs         # Logic tương tác database
  ├── Cargo.toml
  └── .env.example      # Chứa biến môi trường mẫu (DATABASE_URL)
  ```
- **Database Schema (Ví dụ migration ban đầu):**
  ```sql
  -- migrations/<timestamp>_initial_schema.sql
  CREATE TABLE applications (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name VARCHAR(255) UNIQUE NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
  );

  CREATE TABLE deployments (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      app_id UUID NOT NULL REFERENCES applications(id),
      artifact_url VARCHAR(1024) NOT NULL,
      status VARCHAR(50) NOT NULL, -- e.g., 'pending', 'running', 'failed'
      created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
  );
  ```
- **State Management trong Axum:**
  ```rust
  // Ví dụ trong src/main.rs
  use axum::extract::FromRef;
  use sqlx::PgPool;

  #[derive(Clone, FromRef)]
  struct AppState {
      db_pool: PgPool,
  }
  // ... khởi tạo pool và truyền vào router
  let app = Router::new()
      .route("/health", get(health_check))
      .with_state(app_state);
  ```

### 4. Yêu cầu về Kiểm thử (Testing Requirements)
- **Unit Tests:**
  - [ ] Viết test cho các hàm helper hoặc logic phức tạp trong các module (nếu có).
  - [ ] Test các hàm trong module `db` với một database test (sử dụng `sqlx`'s test macros).
- **Integration Tests:**
  - [ ] Viết test tích hợp cho từng API endpoint.
  - [ ] Sử dụng `axum::http::Request` và `tower::ServiceExt` để gọi các handler.
  - [ ] Test endpoint `/health` trả về đúng status và body.
  - [ ] Test các endpoint mock (`/deployments`, `/apps`, etc.) trả về đúng status code.
  - [ ] Test các trường hợp lỗi (ví dụ: request body không hợp lệ) và xác minh server trả về lỗi 4xx phù hợp.
- **Kiểm thử Thủ công:**
  - [ ] Chạy `sqlx migrate run` để áp dụng migration.
  - [ ] Chạy `cargo run` để khởi động server.
  - [ ] Sử dụng `curl` hoặc một API client để gọi các endpoint và xác minh kết quả.
    - `curl http://localhost:3000/health`
    - `curl -X POST http://localhost:3000/deployments -H "Content-Type: application/json" -d '{}'`
