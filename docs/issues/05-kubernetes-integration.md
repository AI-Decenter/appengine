# Issue #5: Tích hợp Kubernetes và logic Deploy

**Tên Issue:** 🚀 [FEAT] - Control Plane: Tích hợp Kubernetes và xử lý yêu cầu Deploy

**Nhãn:** `enhancement`, `control-plane`, `kubernetes`, `epic`

**Người thực hiện:** (Để trống)

---

### 1. Mô tả (Description)
Đây là issue tích hợp quan trọng, nơi Control Plane thực sự "nói chuyện" với Kubernetes. Chúng ta sẽ sử dụng crate `kube-rs` để:
1.  Lắng nghe yêu cầu `POST /deployments` từ Aether CLI.
2.  Lưu thông tin deployment vào cơ sở dữ liệu.
3.  Tạo một tài nguyên `Deployment` và `Service` tương ứng trên cluster Kubernetes.

Logic này sẽ bao gồm việc tạo một `PodSpec` tùy chỉnh với `initContainer` để tải và giải nén artifact ứng dụng.

### 2. Tiêu chí Hoàn thành (Definition of Done)
- [ ] Thư viện `kube` và `k8s-openapi` được thêm vào `control-plane/Cargo.toml`.
- [ ] Control Plane có thể khởi tạo một Kubernetes client và kết nối tới cluster (sử dụng cấu hình từ `~/.kube/config` cho môi trường dev).
- [ ] Handler cho `POST /deployments` được cập nhật để:
    - Nhận thông tin về artifact (ví dụ: URL của `app.tar.gz`).
    - Tạo một bản ghi mới trong bảng `deployments` với trạng thái `pending`.
    - Tạo một `k8s_openapi::api::apps::v1::Deployment` mới.
    - Tạo một `k8s_openapi::api::core::v1::Service` để expose ứng dụng.
- [ ] `PodSpec` của `Deployment` phải được cấu hình chính xác:
    - **`initContainers`**: Một container sử dụng image như `busybox` hoặc `alpine` để chạy lệnh `wget` hoặc `curl` tải artifact từ URL, sau đó dùng `tar` để giải nén vào một `emptyDir` volume.
    - **`containers`**: Container chính của ứng dụng, sử dụng base image `aether-nodejs:20-slim`.
    - **`volumes`**: Một `emptyDir` volume được định nghĩa và mount vào cả `initContainer` và container chính.
- [ ] Sau khi tạo tài nguyên K8s thành công, trạng thái của deployment trong DB được cập nhật thành `running`.
- [ ] Nếu có lỗi khi tương tác với K8s API, lỗi đó phải được log lại và trả về một response 500 cho client.

### 3. Thiết kế & Kiến trúc (Design & Architecture)
- **Kubernetes Client trong Axum State:**
  - Khởi tạo K8s client và thêm nó vào `AppState` để các handler có thể truy cập.
- **Tạo tài nguyên với `kube-rs`:**
  ```rust
  // Ví dụ logic tạo Deployment
  use kube::{Api, Client, api::{PostParams, ResourceExt}};
  use k8s_openapi::api::apps::v1::Deployment;

  async fn create_deployment(client: Client, data: &MyDeploymentData) -> Result<(), kube::Error> {
      let deployments: Api<Deployment> = Api::default_namespaced(client);
      let deployment_manifest: Deployment = serde_yaml::from_str(&build_yaml(data))?; // Hàm helper để tạo YAML

      deployments.create(&PostParams::default(), &deployment_manifest).await?;
      Ok(())
  }
  ```
- **Cấu trúc YAML của Pod (được tạo bằng code Rust):**
  ```yaml
  # Đây là cấu trúc YAML mục tiêu mà code Rust sẽ tạo ra
  apiVersion: apps/v1
  kind: Deployment
  metadata:
    name: my-nodejs-app
  spec:
    replicas: 1
    template:
      spec:
        volumes:
          - name: app-code
            emptyDir: {}
        initContainers:
          - name: fetch-artifact
            image: busybox:1.35
            command:
              - 'sh'
              - '-c'
              - 'wget -O - http://minio/artifacts/app.tar.gz | tar -xz -C /app'
            volumeMounts:
              - name: app-code
                mountPath: /app
        containers:
          - name: app-container
            image: aether-nodejs:20-slim
            command: ["npm", "start"]
            workingDir: /app
            volumeMounts:
              - name: app-code
                mountPath: /app
  ```

### 4. Yêu cầu về Kiểm thử (Testing Requirements)
- **Unit Tests:**
  - [ ] Viết test cho hàm tạo manifest YAML/JSON của `Deployment` và `Service` để đảm bảo tất cả các trường đều chính xác.
- **Integration Tests:**
  - Đây là phần khó test nhất. Một cách tiếp cận là sử dụng một K8s client giả (mock).
  - [ ] Viết test tích hợp cho endpoint `/deployments` mà không thực sự gọi K8s API, chỉ xác minh rằng logic nghiệp vụ (tạo bản ghi DB) hoạt động đúng.
- **End-to-End (E2E) / Kiểm thử Thủ công:**
  - **Đây là phần kiểm thử quan trọng nhất cho issue này.**
  - [ ] Chạy Control Plane cục bộ.
  - [ ] Đảm bảo Minikube đang chạy.
  - [ ] Sử dụng `curl` để gửi một yêu cầu `POST /deployments` hợp lệ (với một URL artifact giả).
  - [ ] Dùng `kubectl get deployments,pods,services -w` để theo dõi việc tạo tài nguyên trong thời gian thực.
  - [ ] Kiểm tra log của `initContainer` để xem nó có tải và giải nén thành công không.
  - [ ] Kiểm tra log của container ứng dụng chính.
  - [ ] Dùng `kubectl delete deployment <name>` để dọn dẹp.
