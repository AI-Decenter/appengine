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

````