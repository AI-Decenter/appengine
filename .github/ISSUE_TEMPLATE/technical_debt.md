---
name: "🛠️ Nợ kỹ thuật / Tái cấu trúc (Tech Debt / Refactoring)"
about: "Theo dõi các công việc cải thiện mã nguồn, cơ sở hạ tầng hoặc các khoản nợ kỹ thuật khác."
title: "🛠️ [REFACTOR] - <Mô tả ngắn gọn về công việc>"
labels: ["technical-debt"]
assignees: ''

---

### Mô tả công việc (Task Description)
Mô tả rõ ràng về nợ kỹ thuật cần giải quyết hoặc phần mã nguồn cần tái cấu trúc. Giải thích tại sao nó cần được cải thiện.

### Lý do (Rationale)
Tại sao việc này lại quan trọng? Nó sẽ cải thiện những khía cạnh nào (ví dụ: hiệu năng, khả năng bảo trì, giảm độ phức tạp)?

### Kế hoạch thực hiện (Implementation Plan)
Đề xuất các bước để giải quyết công việc này.
1. **Phân tích:**
2. **Tái cấu trúc:**
3. **Kiểm thử:**

### Tiêu chí hoàn thành (Definition of Done)
- [ ] Mã nguồn được tái cấu trúc thành công.
- [ ] Tất cả các bài kiểm thử hiện có đều vượt qua.
- [ ] Các bài kiểm thử mới (nếu cần) được thêm vào để đảm bảo không có hồi quy (regression).
- [ ] Hiệu năng không bị suy giảm (hoặc được cải thiện).
- [ ] Tài liệu liên quan được cập nhật.

### Yêu cầu về Kiểm thử (Testing Requirements)
- [ ] Đảm bảo độ bao phủ của Unit Test không giảm sau khi tái cấu trúc.
- [ ] Chạy lại toàn bộ Integration Tests để xác nhận không có tác dụng phụ.
- [ ] Thực hiện kiểm thử hồi quy (regression testing) trên các khu vực bị ảnh hưởng.
