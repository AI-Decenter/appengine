# Issue #1: Thiết lập nền tảng cho Aether CLI

**Tên Issue:** 🚀 [FEAT] - Nền tảng Aether CLI và các lệnh cơ bản

**Nhãn:** `enhancement`, `cli`, `epic`

**Người thực hiện:** (Để trống)

---

## 1. Mô tả (Description)
Xây dựng nền tảng vững chắc, tối ưu và dễ mở rộng cho `aether-cli`, công cụ dòng lệnh trung tâm của hệ sinh thái AetherEngine. Pha này tập trung vào khung (scaffolding) chất lượng cao: cấu trúc module rõ ràng, chuẩn lỗi & logging thống nhất, test đầy đủ, đảm bảo hiệu năng khởi động nhanh, và sẵn sàng tích hợp dần backend (Control Plane + Artifact Registry) ở các issue tiếp theo.

## 2. Phạm vi & Không thuộc phạm vi (Scope / Out of Scope)
Phạm vi:
- Parsing lệnh và tiền xử lý (argument validation, config load).
- Mock logic cho các subcommand: `login`, `deploy`, `logs`, `list` (chỉ in thông báo chuẩn hoá).
- Hệ thống config + token store nội bộ (mock lưu file cục bộ, chưa mã hoá phức tạp ở giai đoạn này).
- Logging có cấu trúc & màu sắc (human) + JSON (tùy chọn qua flag `--log-format json`).
- Sinh shell completions (bash, zsh, fish) qua subcommand ẩn.

Không thuộc phạm vi (sẽ ở issue sau):
- Gọi thật API Control Plane.
- Upload artifact và build NodeJS thật.
- Streaming logs từ Kubernetes.
- Telemetry usage/metrics (chỉ đặt chỗ optional flag, chưa gửi dữ liệu).

## 3. Definition of Done (Mở rộng)
- [x] Crate `aether-cli` tồn tại & build qua `cargo build --workspace`.
- [x] Có module `commands` tách riêng mỗi subcommand một file.
- [x] Subcommands tối thiểu: `login`, `deploy`, `logs`, `list`, và `completions` (ẩn / documented-hidden).
- [x] `--version`, `--help` hoạt động & được test.
- [x] Flag chung: `--log-level <trace|debug|info|warn|error>` (mặc định: info), `--log-format <auto|text|json>`.
- [x] Thư mục cấu hình: `${XDG_CONFIG_HOME:-~/.config}/aether/config.toml` được đọc nếu tồn tại (có env override `AETHER_DEFAULT_NAMESPACE`).
- [x] Token đăng nhập được lưu tại: `${XDG_CACHE_HOME:-~/.cache}/aether/session.json` (mock token JSON), cảnh báo nếu quyền file quá mở (> 0600 trên Unix) + test forcing warning.
- [x] Code an toàn: không panic ngoài test; xử lý lỗi bằng `anyhow` + scaffold `thiserror` + exit code mapping (generic runtime -> 20; further granular mapping future work).
- [x] Exit codes chuẩn hoá: 0 (success), 2 (usage error via clap), 10 (config error), 20 (runtime internal mock), 30 (I/O/FS placeholder), 40 (network placeholder) – basic mapping implemented.
- [x] Logging: mỗi subcommand tạo span + log start/end với duration ms.
- [x] Thời gian khởi động test (CI relaxed) – performance test asserts <800ms; local target <150ms (manual đo cần bổ sung số liệu thực tế).
- [ ] `cargo clippy -- -D warnings` sạch (pending verification run).
- [ ] `cargo deny check` pass (pending run; expected pass—licenses already normalized).
- [ ] Test coverage logic commands ≥ 80% (ước lượng – cần chạy `cargo llvm-cov` nếu tích hợp; chưa đo tự động).
- [x] Tạo tài liệu usage tối thiểu trong README (đã cập nhật phần CLI, exit codes, ignore file, examples).

## 4. Thiết kế & Kiến trúc
### 4.1 Cấu trúc thư mục
```
crates/aether-cli/
├── src/
│   ├── main.rs              # entrypoint: parse args, init logger, dispatch
│   ├── lib.rs               # re-export types, shared helpers
│   ├── config.rs            # load/merge config + constants paths
│   ├── logging.rs           # setup tracing subscriber (text/json)
│   ├── errors.rs            # domain error + exit code mapping
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── login.rs
│   │   ├── deploy.rs
│   │   ├── logs.rs
│   │   ├── list.rs
│   │   └── completions.rs   # generate shell completion scripts
│   └── util/
│       └── time.rs          # duration formatting helper
└── tests/
    └── cli_basic.rs
```

### 4.2 Mô hình lệnh (Command Model)
```rust
#[derive(Parser)]
#[command(author, version, about = "AetherEngine CLI", long_about = None)]
pub struct Cli {
    /// Định dạng log: auto|text|json
    #[arg(long, default_value = "auto")]
    pub log_format: LogFormat,

    /// Mức log: trace|debug|info|warn|error
    #[arg(long, default_value = "info")]
    pub log_level: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Đăng nhập (mock) và lưu token cục bộ
    Login { #[arg(long)] username: Option<String> },
    /// (Mock) đóng gói và triển khai ứng dụng NodeJS trong thư mục hiện tại
    Deploy { #[arg(long)] dry_run: bool },
    /// (Mock) hiển thị 10 dòng log cuối (giả lập)
    Logs { #[arg(long)] app: Option<String> },
    /// (Mock) liệt kê ứng dụng đã triển khai (giả lập)
    List {},
    /// Sinh shell completion (ẩn)
    #[command(hide = true)]
    Completions { #[arg(long, default_value = "bash")] shell: String },
}
```

### 4.3 Config Merge Order
1. Mặc định nội bộ (hard-coded).
2. File `config.toml` (nếu tồn tại).
3. Biến môi trường `AETHER_*` (ghi đè file).
4. Tham số dòng lệnh (ghi đè tất cả).

### 4.4 Logging
- Dùng `tracing` + `tracing-subscriber`.
- Format text: thời gian tương đối, màu, target tối giản.
- Format JSON: trường chuẩn `ts, level, message, span, duration_ms`.
- Tự động thêm span cho từng subcommand.

### 4.5 Error Handling & Exit Codes
Lớp error domain (nếu cần): `CliError { kind: CliErrorKind, source: Option<anyhow::Error> }`.
Map sang exit code thông qua `impl CliErrorKind { fn code(&self)->i32 }`.

### 4.6 Bảo mật (Security Considerations)
- Token mock lưu file JSON quyền 0600; cảnh báo nếu khác.
- Không in token trong log.
- Không panic với input người dùng.
- Chuẩn bị sẵn hook để mã hoá (future: sử dụng OS keyring).

### 4.7 Hiệu năng (Performance)
- Parse + init logger + dispatch < 150ms cold start.
- Không load file lớn hay scan đệ quy ở pha nền tảng.

### 4.8 Khả năng mở rộng (Extensibility)
- Thêm subcommand mới chỉ cần tạo file mới + khai báo enum.
- Shared context struct (future) có thể thêm vào mà không phá vỡ API public (crate internal).

## 5. Kế hoạch Thực hiện (Implementation Plan)
1. Tạo module skeleton + enums.
2. Thêm logging setup (text + json).
3. Implement config loader.
4. Implement token store mock.
5. Implement từng subcommand (mock body).
6. Thêm completions generator.
7. Viết unit tests cho parsing + helper.
8. Viết integration tests bằng `assert_cmd`.
9. Thêm clippy + deny vào CI (đã có) đảm bảo pass.
10. Cập nhật README usage.

## 6. Ma trận Kiểm thử (Test Matrix)
| Trường hợp | Mô tả | Kỳ vọng |
|------------|-------|---------|
| `--help` | Hiển thị trợ giúp | chứa các subcommand |
| `--version` | Hiển thị phiên bản | version khớp Cargo.toml |
| `login` không tham số | tạo token mock | file session.json tồn tại |
| `login --username foo` | lưu username | file chứa username |
| `deploy --dry_run` | không tạo artifact thật | log có "dry run" |
| `logs` | in log giả | >=1 dòng mock |
| `list` | in danh sách mock | văn bản chứa tiêu đề |
| `--log-format json` | JSON đúng | parse được JSON |
| Config file + env override | ưu tiên đúng thứ tự | giá trị cuối cùng đúng |
| Permission file token >0600 | cảnh báo | warning xuất hiện |

## 7. Yêu cầu về Kiểm thử (Testing Requirements)
### 7.1 Unit Tests
- [ ] Parsing: từng combination flags cơ bản.
- [ ] Log format enum parse.
- [ ] Path resolution XDG vs macOS/Linux fallback.
- [ ] Token store write/read roundtrip (temp dir).
- [ ] Exit code mapping.

### 7.2 Integration Tests (`tests/`)
- [ ] `--help`, `--version`.
- [ ] `login` (idempotent: chạy 2 lần không crash).
- [ ] `deploy --dry_run` trả về exit 0.
- [ ] `logs`, `list` không lỗi.
- [ ] `--log-format json` output hợp lệ (dòng đầu parse được JSON).

### 7.3 (Optional) Property Tests
- [ ] Arbitrary chuỗi username hợp lệ -> không panic.

### 7.4 Manual Acceptance
- [ ] Đo thời gian: `time target/debug/aether-cli --help`.
- [ ] Kiểm tra completions: `aether-cli completions --shell bash` sinh nội dung.
- [ ] Thử xóa file token rồi `login` lại.

## 8. Rủi ro & Giảm thiểu (Risks & Mitigations)
| Rủi ro | Ảnh hưởng | Giảm thiểu |
|--------|-----------|-----------|
| Thiết kế kém dẫn tới khó mở rộng | Chậm giai đoạn sau | Module hoá + review kiến trúc sớm |
| Lạm dụng unwrap/panic | Crash CLI | Dùng anyhow + map_err consistent |
| Logging nhiễu | Khó đọc | Cấp độ log điều chỉnh được |
| File quyền rộng | Rò rỉ token | Kiểm tra & cảnh báo |

## 9. Chỉ số Chấp nhận (Acceptance Metrics)
- Tất cả checklist DoD ✓.
- 100% bài test định nghĩa trong ma trận pass.
- Không còn cảnh báo clippy.
- `cargo deny check` pass.
- Manual performance dưới ngưỡng.

## 10. Theo dõi (Tracking Checklist)
- [x] Scaffolding crate
- [x] Commands enum + dispatcher
- [x] Logging subsystem
- [x] Config loader (incl. env override)
- [x] Token store mock
- [x] Implement login
- [x] Implement deploy (mock + artifact + ignore patterns)
- [x] Implement logs (mock)
- [x] Implement list (mock)
- [x] Completions command
- [x] Unit / integration tests (parsing, deploy, ignore, permissions, json logs, performance)
- [ ] Optional property tests (chưa thực hiện – optional)
- [x] README update
- [x] Performance check (automated relaxed test; manual fine-grain measurement TBD)
- [ ] Final review & squash (pending after clippy/deny & coverage note)

---
Ghi chú: Đây là nền tảng – ưu tiên rõ ràng, sạch, dễ mở rộng. Không tối ưu premature ngoại trừ phần khởi động & UX cơ bản.
