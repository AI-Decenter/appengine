````markdown
# Issue 05: Dev Hot Reload (Sidecar Fetch Loop)

## Ý tưởng
Cho phép lập trình viên cập nhật code mà không rebuild image: sidecar fetch định kỳ artifact mới nếu digest annotation thay đổi.

## Scope
* Sidecar container `fetcher` (alpine + curl + tar) loop: nếu annotation digest khác local -> tải & giải nén vào EmptyDir.
* Lệnh CLI: `aether deploy --dev-hot` -> sau upload patch deployment annotation `digest=<new>`.
* Graceful reload Node: gửi `SIGUSR2` hoặc restart process (option simple: process auto reload on file change via `nodemon`).

## Acceptance
| ID | Mô tả | Kết quả |
|----|------|---------|
| H1 | Patch digest |  ≤10s code mới có hiệu lực |
| H2 | Digest không đổi | Không tải lại |

## Test
* Script dev.sh subcommand mô phỏng patch annotation.

````