````markdown
# Issue 10: Auth & RBAC Foundation

## Scope
* Token tĩnh cấu hình qua env `AETHER_API_TOKENS` (CSV).  
* Middleware parse bearer token -> context user.
* Bảng `users` (id, name?, created_at, token_hash?).
* RBAC mức thô: role `admin` (full) / `reader`.

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| A1 | Request thiếu token | 401 |
| A2 | Token hợp lệ | 200 |
| A3 | Role reader tạo deploy | 403 |

## Tasks (checklist)

- [ ] Cấu hình & hợp đồng ENV
	- Định nghĩa biến `AETHER_API_TOKENS` (CSV), format đề xuất: `token:role[:name]`.
		- Ví dụ: `AETHER_API_TOKENS="t_admin:admin:alice,t_reader:reader:bob"`
		- Vai trò hợp lệ: `admin`, `reader` (mở rộng sau này: `writer`, …)
	- Tùy chọn: `AETHER_AUTH_REQUIRED=1` (mặc định bật); `AETHER_AUTH_LOG_LEVEL=warn`

- [ ] Data model & migration (users)
	- Bảng `users`:
		- `id UUID PK`
		- `name TEXT NULL`
		- `role TEXT NOT NULL CHECK (role IN ('admin','reader'))`
		- `token_hash TEXT UNIQUE NULL` (SHA-256 hex) — optional seed từ ENV
		- `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`
	- Tạo migration: `crates/control-plane/migrations/2025XXXXXX_create_users.sql` (up/down)
	- Seed tùy chọn (in-memory từ ENV, không buộc phải ghi DB ở bước đầu)

- [ ] Middleware Bearer token (Axum)
	- Tách `Authorization: Bearer <token>`
	- Map token → `UserContext { user_id (uuid v5 từ token), name?, role }`
	- Lookup thứ tự ưu tiên: in-memory map từ ENV (O(1)); fallback (tuỳ chọn) DB `users.token_hash`
	- Constant-time so sánh token (tránh timing hint) — dùng so sánh theo độ dài + `subtle` hoặc so khớp SHA-256
	- Trả 401 khi: vắng header, sai schema, token không hợp lệ
	- Không log token thô; chỉ log hash-prefix (ví dụ 6 ký tự đầu của sha256)

- [ ] RBAC guard (policy)
	- Helper `require_role(min_role)` với thứ tự `admin > reader`
	- Áp dụng:
		- Tạo deployment (POST /deployments) → yêu cầu `admin` (A3=403 khi reader)
		- Các GET/health/status → `reader` (hoặc công khai tùy endpoint)
	- Trả 403 khi token hợp lệ nhưng thiếu quyền

- [ ] Wiring vào router (control-plane)
	- Đăng ký middleware auth vào các nhánh API cần bảo vệ
	- Xác định danh sách route write: artifacts presign/complete, deployments create, …
	- Cho phép bỏ qua auth khi `AETHER_AUTH_REQUIRED=0` (dev/test nhanh)

- [ ] Unit/Integration tests (đáp ứng A1–A3)
	- A1: Không gửi header → 401
	- A2: Header với `t_admin` → 200 trên route GET/health/hoặc danh sách
	- A3: Dùng `t_reader` gọi POST /deployments → 403
	- Test parse ENV CSV, case không hợp lệ bị bỏ qua an toàn
	- Test constant-time compare (khói) — bảo đảm logic không rò rỉ qua nhánh rõ ràng

- [ ] Observability & logs
	- Thêm field trace `user.role`, `user.name?`, `auth.result`
	- Rate limit log 401 (chỉ cảnh báo, không spam)

- [ ] Tài liệu & ví dụ sử dụng
	- README (control-plane): cách đặt `AETHER_API_TOKENS`, ví dụ curl với Bearer
	- Cảnh báo bảo mật: không commit token thực, chỉ dùng env/secret store

- [ ] CI wiring tối thiểu
	- Thêm `AETHER_API_TOKENS` dummy vào job test Linux để chạy integration auth
	- Đảm bảo không in ra token thô trong log CI

## Thiết kế nhanh

- Nguồn nhận dạng: token static qua ENV → map in-memory `HashMap<String, UserInfo>`; khởi tạo khi boot
- Bảo mật token:
	- So sánh constant-time: so sánh 2 chuỗi theo byte, không early-return; hoặc so hash SHA-256
	- Không log token; chỉ log hash prefix (sha256(token)[..6]) khi cần debug
- UserContext:
	- `{ user_id: Uuid, role: Role, name: Option<String> }` (Uuid v5 dựa trên token để ổn định nhưng không lộ token)

## Migration (phác thảo)

Up:
```sql
CREATE TABLE IF NOT EXISTS users (
	id UUID PRIMARY KEY,
	name TEXT NULL,
	role TEXT NOT NULL CHECK (role IN ('admin','reader')),
	token_hash TEXT UNIQUE NULL,
	created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Down:
```sql
DROP TABLE IF EXISTS users;
```

## Test kế hoạch (chi tiết)

- Unit: parse `AETHER_API_TOKENS` → vector (token, role, name?) và map
- Unit: constant-time compare helper
- Integration (Axum):
	- spin server với `AETHER_API_TOKENS=t_admin:admin:alice,t_reader:reader:bob`
	- GET /health (không yêu cầu?) → 200
	- GET /deployments (reader) → 200
	- POST /deployments (reader) → 403
	- POST /deployments (admin) → 200
	- Missing header trên route yêu cầu auth → 401

## Plan & timeline (1 sprint ~ 1 tuần)

- Ngày 1: Thiết kế ENV + middleware skeleton, helper compare
- Ngày 2: Migration users + wiring router các route chính
- Ngày 3: RBAC guard + áp dụng vào deployments/artifacts write
- Ngày 4: Unit tests + Integration A1–A3
- Ngày 5: Observability/logs + README
- Ngày 6: CI wiring + làm sạch log
- Ngày 7: Buffer/bake & chỉnh sửa theo feedback

## Definition of Done

- Middleware auth hoạt động, trả về đúng A1/A2/A3
- Ít nhất 1 integration test phủ A1–A3 chạy trên CI
- Docs hướng dẫn ENV và ví dụ curl
- Logs không rò rỉ token; chỉ hash prefix nếu bật debug
- Có migration `users` (chưa cần seed DB bắt buộc)

## Progress (Oct 11, 2025)

- Implemented Bearer auth middleware with two modes:
	- env mode: AETHER_AUTH_ENABLED=1, tokens via AETHER_ADMIN_TOKEN and AETHER_USER_TOKEN
	- db mode: AETHER_AUTH_MODE=db, lookup users.token_hash (sha256) and role
- Router wiring: public endpoints open; protected endpoints require auth; admin-only on POST /deployments and PATCH /deployments/:id
- TDD: Added integration tests covering A1–A3 for env and db modes; tests pass locally
- Migration added: 202510110001_add_users_auth.sql
- Docs: README in control-plane includes quick env and example

## Rủi ro & mở rộng

- Tạm thời token static qua ENV; về sau có thể chuyển qua DB/issuer JWT/OIDC
- Có thể thêm role `writer` và matrix chi tiết hơn
- Secret quản lý qua GitHub Actions secrets/ KMS/ Vault (không commit vào repo)

````