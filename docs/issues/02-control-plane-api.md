# Issue #2: Xây dựng Control Plane API cơ bản

**Tên Issue:** 🚀 [FEAT] - Nền tảng Control Plane API với Axum

**Nhãn:** `enhancement`, `control-plane`, `api`, `epic`

**Người thực hiện:** (Để trống)

---

### 1. Mô tả (Description)
Issue này tập trung vào việc xây dựng "bộ não" của AetherEngine - Control Plane. Chúng ta sẽ tạo một dịch vụ web bằng Axum, định nghĩa các API endpoint chính, và thiết lập kết nối ban đầu tới cơ sở dữ liệu PostgreSQL sử dụng `sqlx`.

### 2. Tiêu chí Hoàn thành (Definition of Done)
- [x] Một crate Rust mới tên là `control-plane` được tạo trong workspace.
- [x] Các thư viện `axum`, `tokio`, `serde`, `sqlx` được thêm vào (đang dùng `runtime-tokio` + `postgres` qua workspace). (Ghi chú: chưa bật `rustls`; có thể bổ sung nếu cần TLS native) 
- [x] Một web server Axum cơ bản có thể khởi động và chạy (`cargo run -p control-plane`).
- [x] API endpoint `GET /health` trả về `200 OK` + body JSON `{"status": "ok"}`.
- [x] Thiết lập migration ban đầu. (Ghi chú: migration hiện tại mở rộng hơn spec — đã thêm các bảng bổ sung bên cạnh `applications` và `deployments`).
- [x] Cấu hình kết nối database qua biến môi trường `DATABASE_URL` (có file `.env.example`).
- [x] Logic khởi tạo state (struct `AppState { db: Option<Pool<Postgres>> }`) và chia sẻ vào router.
- [x] API endpoint mock cốt lõi:
  - `POST /deployments`: Body JSON rỗng, trả về `201 Created` + id giả lập.
  - `GET /apps`: Trả về `[]`.
  - `GET /apps/{app_name}/logs`: Trả về chuỗi rỗng.
- [x] Endpoint `GET /readyz` (bổ sung so với spec gốc) trả về trạng thái sẵn sàng đơn giản.
- [x] Mã nguồn được cấu trúc module: `handlers/`, `models.rs`, `db.rs`.

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
  - [x] Test logic DB thực tế (create/list applications, create/list deployments, xung đột tên application, truy vấn deployments theo app).
  - [x] (Ở mức tối thiểu) Các handler đơn giản được kiểm tra thông qua integration tests.
- **Integration Tests:**
  - [x] Test cho từng API endpoint hiện có (/health, /readyz, /deployments, /apps, /apps/{app}/logs).
  - [x] Dùng `tower::ServiceExt::oneshot` để mô phỏng request.
  - [x] `/health` trả về đúng status & body JSON.
  - [x] Các endpoint mock trả về đúng status code & payload rỗng.
  - [x] Trường hợp lỗi: body JSON không hợp lệ ở `POST /deployments` → 400.
- **Kiểm thử Thủ công:**
  - [x] Server khởi chạy được (địa chỉ mặc định: `0.0.0.0:3000`).
  - [x] Thực thi `sqlx migrate run` với Postgres cục bộ / trong CI (được chạy trong job `test-linux`).
  - [x] Có thể gọi thử bằng `curl` (hướng dẫn giữ nguyên ở dưới):
   - `curl http://localhost:3000/health`
   - `curl -X POST http://localhost:3000/deployments -H "Content-Type: application/json" -d '{}'`

### 5. Hướng Phát Triển Tiếp Theo (Future Enhancements)
1. Hoàn thiện Tầng Dữ Liệu:
  - Thêm thao tác thật cho `POST /deployments` (ghi vào bảng deployments, validate app tồn tại).
  - Hiện thực `GET /apps` đọc từ DB thay vì trả về mảng rỗng.
  - Thêm endpoint tạo application (`POST /apps`).
  - Chuẩn hoá migration: tách phần mở rộng (artifacts, events) thành các migration kế tiếp có mô tả rõ ràng.
2. Quan Sát & Vận Hành (Observability):
  - Tích hợp `tracing` spans giàu ngữ cảnh (deployment_id, app_name).
  - Thêm metrics (Prometheus exporter) / endpoint `/metrics`.
  - Health nâng cao: `/readyz` kiểm tra DB (ping) & pending migrations.
3. Bảo Mật & Quản Trị:
  - Thêm auth (token-based / OIDC) và RBAC đơn giản (role: reader/operator/admin).
  - Rate limiting / request idempotency (đặc biệt cho tạo deployment).
4. API Contract & DX:
  - Sinh OpenAPI/Swagger (ví dụ: `utoipa` hoặc `okapi`).
  - Thêm client SDK (Rust / TypeScript) tự động từ spec.
  - Kiểm tra contract bằng chai kiểm thử (schematest / Prism).
5. Chất Lượng & CI/CD:
  - Kích hoạt `sqlx` offline cache (`sqlx prepare`) + check trong CI.
  - Thêm test database ephemeral (Docker) cho các bài test DB thực sự.
  - Benchmark cơ bản cho throughput của `/deployments`.
6. Kiến Trúc & Mở Rộng:
  - Chuyển `AppState.db` từ `Option` sang bắt buộc sau khi giai đoạn mock kết thúc.
  - Thêm lớp service tách khỏi handler để dễ unit test logic nghiệp vụ.
  - Giới thiệu pattern Event (deployment events) + publish sang message bus (Kafka/NATS) trong bước sau.
7. Khả Năng Tin Cậy:
  - Graceful shutdown (drain in-flight requests, close pool).
  - Health phân tầng: `/health` (liveness), `/readyz` (db + external deps), `/startupz` (migrations run xong).
8. Bảo Mật Kết Nối:
  - Bật feature `runtime-tokio-rustls` cho `sqlx` / TLS connection tới Postgres nếu dùng managed service.
9. Logging & Error Handling:
  - Chuẩn hoá error JSON: `{ "error": { "code": "...", "message": "..." } }`.
  - Map lỗi sqlx thành mã lỗi 404 / 409 phù hợp.
10. Khác:
  - Thêm test fuzz (serde payload) cho endpoints.
  - Thêm limit kích thước body (Layer) & timeout layer.
  - Thêm CORS config nếu front-end sẽ truy cập trực tiếp.

### 6. Ghi chú Trạng thái Hiện Tại
Hoàn tất Definition of Done ban đầu và đã mở rộng thêm nhiều phần ngoài phạm vi mô tả gốc.

Trạng thái cập nhật:
1. CRUD cơ bản thực thi thật với Postgres:
  - `POST /apps` tạo application (xử lý xung đột 409).
  - `GET /apps` truy vấn danh sách thật từ DB.
  - `POST /deployments` tạo deployment gắn với application tồn tại.
  - `GET /deployments` & `GET /apps/:app/deployments` liệt kê dữ liệu thật.
  - `GET /apps/:app/logs` vẫn placeholder (trả rỗng) – intentional.
2. Migration đã được chuẩn hoá xuống chỉ còn 2 bảng cốt lõi (`applications`, `deployments`) + trigger cập nhật `updated_at` (nếu có) thay vì schema mở rộng.
3. Đã chuẩn hoá lỗi JSON: mọi lỗi trả về dạng `{ "code": "...", "message": "..." }` (module `error.rs`).
4. Thêm tracing instrumentation (`#[tracing::instrument]`) cho các handler chính (tạo/list apps, tạo/list deployments, truy vấn deployments theo app).
5. Bổ sung test integration bao trùm các tình huống chính: health, readiness, create deployment, list rỗng, flow deployments theo app, xung đột tạo app, JSON không hợp lệ.
6. CI:
  - Thêm workflow chạy Postgres (Linux) + split job macOS (không Docker).
  - Thực thi `sqlx migrate run` & `cargo sqlx prepare` trong CI.
  - Thêm coverage, benchmark, security (cargo-deny) vào pipeline.
7. Đã tạo `sqlx-data.json` (offline prepare) – sẽ tiếp tục mở rộng khi chuyển sang macro nhiều hơn.
8. README có mục mô tả Error Format.
9. Sẵn sàng mở rộng sang OpenAPI & metrics trong bước kế tiếp (chưa thực hiện vì ngoài phạm vi Issue #2 gốc).

Phần còn lại (để lại cho issue khác / future enhancements): OpenAPI spec, metrics `/metrics`, auth/RBAC, rate limiting, body limit/timeout layer, service layer tách biệt, graceful shutdown nâng cao, events bus, CORS.

=> Issue #2 coi như HOÀN THÀNH (completed + extended). 

### 7. Bổ sung đã triển khai ngoài phạm vi ban đầu
- JSON error envelope thống nhất.
- Tracing instrumentation chi tiết.
- Endpoint mở rộng: list deployments toàn cục và theo app.
- Kiểm thử conflict ứng dụng.
- CI multi-platform (Linux DB + macOS no-DB) + coverage + performance benchmark.
- Chuẩn hoá migration tối giản.
- Offline `sqlx prepare` + kiểm tra tự động.
