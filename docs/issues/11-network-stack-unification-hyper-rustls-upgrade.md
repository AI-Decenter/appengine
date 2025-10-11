# Issue 11: Hợp nhất network stack (hyper/h2/http/rustls) & loại bỏ duplicates

## Bối cảnh
Hiện tại đồ thị dependency kéo đồng thời các phiên bản cũ và mới của tầng HTTP/TLS:

| Crate | Legacy | Modern | Ghi chú |
|-------|--------|--------|--------|
| hyper | 0.14.x | 1.7.x | 0.14 kéo theo h2 0.3; 1.x dùng h2 0.4 |
| h2 | 0.3.27 | 0.4.12 | Song song theo hai nhánh hyper |
| http | 0.2.12 | 1.3.1 | Trùng lặp request/response types |
| rustls | 0.21.12 | 0.23.32 | 0.23 yêu cầu cập nhật tokio-rustls mới |
| tokio-rustls | 0.24.1 | 0.26.4 | Khác API nội bộ session config |
| hyper-rustls | 0.24.2 | 0.27.7 | Phân kỳ do chain cũ vẫn tồn tại |

Nguyên nhân chính: các crate transitive (kube 0.88, aws-smithy-runtime, sqlx, reqwest) đang ở giai đoạn chuyển tiếp; một số vẫn tham chiếu hyper 0.14 / rustls 0.21.

## Vấn đề
1. Bloat: Tăng kích thước binary, thời gian build.
2. Bảo mật: Tăng diện tích bề mặt + nhiều đường vá.
3. Công cụ: cargo-deny tạo cảnh báo duplicate; khó bật lại `multiple-versions = "deny"`.
4. Tối ưu: Không tận dụng đầy đủ cải tiến hiệu năng hyper 1.x / h2 0.4.

## Mục tiêu
Hợp nhất sang duy nhất chuỗi hiện đại:
- hyper >= 1.0
- h2 >= 0.4
- http >= 1.0
- rustls >= 0.23
- tokio-rustls >= 0.26
- hyper-rustls >= 0.27

Sau khi hợp nhất: bật lại chặt chẽ `multiple-versions = "deny"` trong `deny.toml`.

## Phạm vi
- Không thay đổi surface public API hiện tại của crates nội bộ.
- Không re-write logic; chỉ nâng cấp phiên bản + điều chỉnh feature flags.
- Theo dõi và (nếu cần) mở issue upstream cho kube / aws-smithy về pinned cũ.

## Kế hoạch thực thi
### Phase 1: Inventory & Theo dõi upstream
- [ ] Tạo tracking links: kube, aws-smithy-runtime, hyper-rustls, sqlx (xác nhận không còn lock cũ).
- [ ] Ghi nhận crate nào còn trực tiếp phụ thuộc hyper 0.14.
- [ ] Kiểm tra MSRV yêu cầu sau nâng cấp rustls 0.23 (hiện workspace đặt 1.90, đủ).

### Phase 2: Cô lập nguồn hyper 0.14
- [ ] Dùng `cargo tree -e features` để thấy feature nào kéo hyper 0.14.
- [ ] Nếu đến từ kube: thử bump phiên bản mới hơn (nếu phát hành) hoặc đề xuất upstream bỏ dependency trực tiếp vào hyper 0.14.
- [ ] Nếu từ crate riêng: chỉnh Cargo.toml trỏ duy nhất hyper 1.x.

### Phase 3: Nâng cấp TLS stack
- [ ] Đảm bảo tất cả phụ thuộc dùng rustls 0.23 / tokio-rustls 0.26.
- [ ] Loại bỏ hyper-rustls 0.24.x còn sót.
- [ ] Chạy regression: kết nối Kubernetes API + S3 upload.

### Phase 4: Siết lại policy
- [ ] Bật lại `multiple-versions = "deny"` trong `[bans]`.
- [ ] Xóa các `bans.skip` lịch sử (nếu còn) và chạy `cargo deny` sạch.

### Phase 5: Tối ưu & Tài liệu
- [ ] Ghi đo lường kích thước binary trước / sau.
- [ ] Cập nhật README / docs: chuẩn network stack.

## Acceptance Criteria
| ID | Mô tả | Điều kiện Pass |
|----|-------|----------------|
| N1 | Không còn hyper 0.14 trong cargo tree | `cargo tree | grep "hyper v0.14"` rỗng |
| N2 | Không còn h2 0.3.x | `cargo tree | grep "h2 v0.3"` rỗng |
| N3 | Chỉ 1 phiên bản rustls (0.23.x) | `cargo tree | grep "rustls v0.21"` rỗng |
| N4 | cargo-deny bans ở chế độ deny không cảnh báo duplicate | `cargo deny check bans` PASS |
| N5 | CI thời gian build giảm (ghi nhận >5% hoặc document nếu không đạt) | So sánh build logs |

## Rủi ro & Giảm thiểu
| Rủi ro | Ảnh hưởng | Giảm thiểu |
|--------|-----------|-----------|
| Upstream chưa phát hành phiên bản bỏ hyper 0.14 | Block hợp nhất | Tạo / link issue upstream, tạm giữ Phase 3 |
| Thay đổi API subtle trong hyper 1.x (feature gating) | Lỗi compile | Incremental nâng cấp, chạy tests mỗi bước |
| rustls 0.23 thay đổi cấu hình ALPN | Lỗi TLS handshake | Test kết nối Kubernetes & S3 staging |

## Công cụ hỗ trợ
- `cargo tree -i <crate>` để truy nguyên ngược.
- `cargo tree -e features` xem feature kích hoạt.
- `cargo udeps` (tùy chọn) kiểm tra deps còn lại sau hợp nhất.
- `scripts/check-network-stack.sh` để fail sớm nếu còn legacy hyper/h2/http/rustls.
- `scripts/measure-build.sh` để đo build time & kích thước binary.

## Artefacts baseline
- docs/issues/11-network-stack-unification-hyper-rustls-upgrade/tree-baseline.txt
- docs/issues/11-network-stack-unification-hyper-rustls-upgrade/versions-grep.txt
- docs/issues/11-network-stack-unification-hyper-rustls-upgrade/binary-sizes-release.txt (sẽ sinh bởi script)
- docs/issues/11-network-stack-unification-hyper-rustls-upgrade/build-time-release.txt (sẽ sinh bởi script)

## Liên kết upstream (dự kiến điền sau)
- [ ] kube issue: (link)
- [ ] aws-smithy-runtime issue: (link)
- [ ] hyper-rustls nâng cấp tracking: (link)

## Follow-up sau khi hoàn thành
1. Bật LTO (nếu chưa) để tận dụng giảm bloat.
2. Cân nhắc bật `panic = abort` cho binary CLI (nếu chấp nhận trade-off backtrace) sau khi stack ổn định.
3. Thiết lập badge CI cho cargo-deny để ngăn tái phát duplicates.

---
Update log (automation):
- Added check script: `scripts/check-network-stack.sh` (legacy guard)
- Added measure script: `scripts/measure-build.sh` (time/size capture)
- Gated AWS S3 deps by feature (control-plane default features now empty) để tránh kéo legacy chain theo mặc định; S3 chỉ bật khi cần với features="s3".

---
Generated on: 2025-09-29

## Cập nhật trạng thái (2025-10-11)

- Baseline artefacts đã được sinh ra: `tree-baseline.txt`, `versions-grep.txt`.
- Đã thêm guard script `scripts/check-network-stack.sh` (CI step) – hiện PASS trong build mặc định (không bật S3) vì toàn bộ stack theo hyper 1.x / h2 0.4 / http 1.x / rustls 0.23 / tokio-rustls 0.26 / hyper-rustls 0.27.
- Control-plane: chuyển `default` features rỗng, `s3` là optional; khi không bật `s3`, đồ thị dependency không kéo legacy.
- Khi bật `--features s3` cho control-plane: vẫn xuất hiện legacy chain từ AWS stack (aws-smithy-http-client hyper-014): hyper 0.14.32, h2 0.3.27, rustls 0.21.12, tokio-rustls 0.24.1, hyper-rustls 0.24.2. Đã cấu hình `aws-config` và `aws-sdk-s3` với `default-features = false` và `features = ["rustls", "rt-tokio"]` để chọn TLS hiện đại khi có thể. Chờ upstream cung cấp connector hyper 1.x.
- Thêm `scripts/measure-build.sh` để đo build time và kích thước binary; sẽ chạy trước/sau hợp nhất để ghi nhận N5.

