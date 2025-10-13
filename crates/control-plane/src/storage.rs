use async_trait::async_trait;
use std::time::Duration;
use tracing::{info,warn};

#[derive(Debug, Clone)]
pub struct PresignedUpload { pub url: String, pub method: String, pub headers: std::collections::HashMap<String,String>, pub storage_key: String }

#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    async fn presign_artifact_put(&self, key:&str, digest:&str, expires:Duration) -> anyhow::Result<PresignedUpload>;
    async fn head_size(&self, key:&str) -> anyhow::Result<Option<i64>>; // None if unknown / not enforced
    async fn head_metadata(&self, key:&str) -> anyhow::Result<Option<std::collections::HashMap<String,String>>>; // metadata (if available)
    /// Optionally compute a remote sha256 for small objects (returns Some(digest) if computed, None if skipped / not supported)
    async fn remote_sha256(&self, _key:&str, _max_bytes: i64) -> anyhow::Result<Option<String>> { Ok(None) }
    /// Multipart operations (default unsupported)
    async fn init_multipart(&self, _key:&str, _digest:&str) -> anyhow::Result<String> { Err(anyhow::anyhow!("multipart unsupported")) }
    async fn presign_multipart_part(&self, _key:&str, _upload_id:&str, _part_number:i32) -> anyhow::Result<PresignedUpload> { Err(anyhow::anyhow!("multipart unsupported")) }
    async fn complete_multipart(&self, _key:&str, _upload_id:&str, _parts:Vec<(i32,String)>) -> anyhow::Result<()> { Err(anyhow::anyhow!("multipart unsupported")) }
}

#[derive(Debug, Clone)]
pub struct MockStorageBackend { pub base_url: String, pub bucket: String }

#[async_trait]
impl StorageBackend for MockStorageBackend {
    async fn presign_artifact_put(&self, key:&str, digest:&str, _expires:Duration) -> anyhow::Result<PresignedUpload> {
        let url = format!("{}/{}/{}", self.base_url.trim_end_matches('/'), self.bucket, key);
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-amz-acl".into(), "private".into());
        headers.insert("x-amz-meta-sha256".into(), digest.to_string());
        Ok(PresignedUpload { url, method: "PUT".into(), headers, storage_key: key.to_string() })
    }
    async fn head_size(&self, _key:&str) -> anyhow::Result<Option<i64>> { Ok(None) } // mock: no remote verification
    async fn head_metadata(&self, _key:&str) -> anyhow::Result<Option<std::collections::HashMap<String,String>>> { Ok(None) }
}

#[cfg(feature="s3")]
#[derive(Clone)]
pub struct S3StorageBackend { client: aws_sdk_s3::Client, bucket: String }

#[cfg(feature="s3")]
impl std::fmt::Debug for S3StorageBackend { fn fmt(&self, f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("S3StorageBackend").field("bucket", &self.bucket).finish() } }

#[cfg(feature="s3")]
#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn presign_artifact_put(&self, key:&str, digest:&str, expires:Duration) -> anyhow::Result<PresignedUpload> {
        use aws_sdk_s3::presigning::PresigningConfig;
        let expires = std::cmp::min(expires.as_secs(), 3600); // cap at 1h
        let config = PresigningConfig::builder().expires_in(Duration::from_secs(expires)).build()?;
    let mut req = self.client.put_object().bucket(&self.bucket).key(key).metadata("sha256", digest);
    if let Ok(sse) = std::env::var("AETHER_S3_SSE") { if !sse.is_empty() {
        #[allow(unused_mut)]
        let mut apply = true;
        #[cfg(feature="s3")]
        {
            use aws_sdk_s3::types::ServerSideEncryption;
            match sse.as_str() {
                "AES256" => { req = req.server_side_encryption(ServerSideEncryption::Aes256); },
                "aws:kms" => { req = req.server_side_encryption(ServerSideEncryption::AwsKms); },
                other => { warn!(value=%other, "invalid_sse_algorithm" ); apply=false; }
            }
        }
        if apply { if let Ok(kms) = std::env::var("AETHER_S3_SSE_KMS_KEY") { if !kms.is_empty() { req = req.ssekms_key_id(kms); } } }
    } }
    let presigned = req.presigned(config).await?;
        let uri = presigned.uri().to_string();
        let mut headers = std::collections::HashMap::new();
    for (k,v) in presigned.headers() { headers.insert(k.to_string(), v.to_string()); }
        Ok(PresignedUpload { url: uri, method: "PUT".into(), headers, storage_key: key.to_string() })
    }
    async fn head_size(&self, key:&str) -> anyhow::Result<Option<i64>> {
        match self.client.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(out)=> Ok(out.content_length()),
            Err(e)=> { warn!(?e, key, "s3_head_object_failed"); Ok(None) }
        }
    }
    async fn head_metadata(&self, key:&str) -> anyhow::Result<Option<std::collections::HashMap<String,String>>> {
        async fn retry_head(client: &aws_sdk_s3::Client, bucket:&str, key:&str) -> anyhow::Result<Option<aws_sdk_s3::operation::head_object::HeadObjectOutput>> {
            let mut attempt = 0u32;
            loop {
                match client.head_object().bucket(bucket).key(key).send().await {
                    Ok(o)=> return Ok(Some(o)),
                    Err(e)=> {
                        attempt +=1;
                        if attempt>=3 { warn!(?e, key, "s3_head_object_failed_final"); return Ok(None); }
                        warn!(?e, attempt, key, "s3_head_object_retry");
                        tokio::time::sleep(Duration::from_millis(50 * 2u64.pow(attempt))).await;
                    }
                }
            }
        }
        if let Some(out) = retry_head(&self.client, &self.bucket, key).await? {
            return Ok(out.metadata().map(|m| m.iter().map(|(k,v)| (k.clone(), v.clone())).collect()));
        }
        Ok(None)
    }

    async fn remote_sha256(&self, key:&str, max_bytes: i64) -> anyhow::Result<Option<String>> {
        // First HEAD (with retry) to inspect size
        let size_opt = self.head_size(key).await?; // already retried in head_size
        let Some(size) = size_opt else { return Ok(None); };
        if size < 0 || size > max_bytes { return Ok(None); }
        // Download and hash
        use sha2::{Sha256, Digest};
        let mut attempt = 0u32;
        loop {
            match self.client.get_object().bucket(&self.bucket).key(key).send().await {
                Ok(obj)=> {
                    let mut hasher = Sha256::new();
                    let mut body = obj.body.into_async_read();
                    // Read fully (size bounded by max_bytes)
                    let mut buf = vec![0u8; 8192];
                    let mut total: i64 = 0;
                    use tokio::io::AsyncReadExt;
                    loop {
                        let n = body.read(&mut buf).await?;
                        if n==0 { break; }
                        total += n as i64;
                        hasher.update(&buf[..n]);
                        if total > max_bytes { return Ok(None); }
                    }
                    if total != size { warn!(key, expected=size, got=total, "remote_sha256_size_mismatch"); }
                    let digest = format!("{:x}", hasher.finalize());
                    return Ok(Some(digest));
                }
                Err(e)=> {
                    attempt +=1;
                    if attempt>=3 { warn!(?e, key, "s3_get_object_failed_final"); return Ok(None); }
                    warn!(?e, attempt, key, "s3_get_object_retry");
                    tokio::time::sleep(Duration::from_millis(75 * 2u64.pow(attempt))).await;
                }
            }
        }
    }
    async fn init_multipart(&self, key:&str, digest:&str) -> anyhow::Result<String> {
        let mut req = self.client.create_multipart_upload().bucket(&self.bucket).key(key).metadata("sha256", digest);
        if let Ok(sse) = std::env::var("AETHER_S3_SSE") { if !sse.is_empty() {
            use aws_sdk_s3::types::ServerSideEncryption;
            match sse.as_str() { "AES256" => { req = req.server_side_encryption(ServerSideEncryption::Aes256); }, "aws:kms" => { req = req.server_side_encryption(ServerSideEncryption::AwsKms); if let Ok(kms)=std::env::var("AETHER_S3_SSE_KMS_KEY") { if !kms.is_empty() { req = req.ssekms_key_id(kms); } } }, _=>{} }
        }}
        let out = req.send().await?; Ok(out.upload_id().unwrap_or_default().to_string())
    }
    async fn presign_multipart_part(&self, key:&str, upload_id:&str, part_number:i32) -> anyhow::Result<PresignedUpload> {
        use aws_sdk_s3::presigning::PresigningConfig;
        let config = PresigningConfig::builder().expires_in(Duration::from_secs(900)).build()?;
        let req = self.client.upload_part().bucket(&self.bucket).key(key).upload_id(upload_id).part_number(part_number);
        let presigned = req.presigned(config).await?;
        let mut headers = std::collections::HashMap::new(); for (k,v) in presigned.headers() { headers.insert(k.to_string(), v.to_string()); }
        Ok(PresignedUpload { url: presigned.uri().to_string(), method: "PUT".into(), headers, storage_key: key.to_string() })
    }
    async fn complete_multipart(&self, key:&str, upload_id:&str, parts:Vec<(i32,String)>) -> anyhow::Result<()> {
        use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(parts.into_iter().map(|(n,e)| CompletedPart::builder().set_part_number(Some(n)).set_e_tag(Some(e)).build()).collect()))
            .build();
        self.client.complete_multipart_upload().bucket(&self.bucket).key(key).upload_id(upload_id).multipart_upload(completed).send().await?; Ok(())
    }
}

#[derive(Clone)]
pub struct StorageManager { inner: std::sync::Arc<dyn StorageBackend> }

impl std::fmt::Debug for StorageManager { fn fmt(&self, f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("StorageManager").finish() } }

impl StorageManager {
    pub async fn from_env() -> Self {
        let mode = std::env::var("AETHER_STORAGE_MODE").unwrap_or_else(|_| "mock".into());
        let bucket = std::env::var("AETHER_ARTIFACT_BUCKET").unwrap_or_else(|_| "artifacts".into());
        // Default to localhost to avoid DNS assumptions like minio.local in most environments
        let base_url = std::env::var("AETHER_S3_BASE_URL").unwrap_or_else(|_| "http://localhost:9000".into());
        if mode.eq_ignore_ascii_case("s3") {
            #[cfg(feature="s3")]
            {
                use aws_config::BehaviorVersion;
                let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".into());
                let shared = aws_config::defaults(BehaviorVersion::latest()).region(aws_sdk_s3::config::Region::new(region.clone())).load().await;
                let mut builder = aws_sdk_s3::config::Builder::from(&shared);
                if let Ok(ep) = std::env::var("AETHER_S3_ENDPOINT_URL") {
                    // Use the provided endpoint (e.g., MinIO) and prefer path-style addressing for compatibility
                    builder = builder.endpoint_url(ep).force_path_style(true);
                }
                let conf = builder.build();
                let client = aws_sdk_s3::Client::from_conf(conf);
                info!(bucket=%bucket, "storage_manager.init_s3");
                return StorageManager { inner: std::sync::Arc::new(S3StorageBackend { client, bucket }) };
            }
            #[cfg(not(feature="s3"))]
            warn!("s3 feature not enabled, falling back to mock backend");
        }
        info!(mode=%mode, bucket=%bucket, "storage_manager.init_mock");
        StorageManager { inner: std::sync::Arc::new(MockStorageBackend { base_url, bucket }) }
    }

    pub fn backend(&self) -> &dyn StorageBackend { self.inner.as_ref() }
}

// Global accessor (lazy)
static STORAGE: once_cell::sync::OnceCell<StorageManager> = once_cell::sync::OnceCell::new();

pub async fn get_storage() -> &'static StorageManager {
    if let Some(s) = STORAGE.get() { return s; }
    let mgr = StorageManager::from_env().await;
    STORAGE.set(mgr).ok();
    STORAGE.get().unwrap()
}
