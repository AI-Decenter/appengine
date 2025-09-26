use clap::{Parser, Subcommand};

pub mod login;
pub mod deploy;
pub mod logs;
pub mod list;
pub mod completions;
pub mod netfail;
pub mod iofail;
pub mod usagefail;
pub mod runtimefail;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogFormat { Auto, Text, Json }

#[derive(Parser, Debug)]
#[command(name = "aether", version, about = "AetherEngine CLI (foundation)")]
pub struct Cli {
    /// Mức log: trace|debug|info|warn|error
    #[arg(long, default_value = "info")]
    pub log_level: String,
    /// Định dạng log: auto|text|json
    #[arg(long, default_value = "auto")]
    pub log_format: LogFormat,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Đăng nhập mock và lưu token local
    Login { #[arg(long)] username: Option<String> },
    /// Build & package ứng dụng NodeJS (npm install + tạo artifact .tar.gz)
    Deploy { 
        #[arg(long, default_value_t = false)] dry_run: bool,
        /// Chỉ đóng gói, bỏ qua bước npm install (hữu ích cho CI không có Node)
        #[arg(long, default_value_t = false)] pack_only: bool,
        /// Mức nén gzip (1-9)
        #[arg(long, default_value_t = 6)] compression_level: u32,
        /// Đường dẫn output (thư mục hoặc file). Nếu là thư mục sẽ tạo tên mặc định app-<sha256>.tar.gz
        #[arg(long)] out: Option<String>,
        /// Bỏ qua bước upload lên Control Plane (dù có AETHER_API_BASE)
        #[arg(long, default_value_t = false)] no_upload: bool,
        /// Vô hiệu cache node_modules (bỏ qua restore/save)
        #[arg(long, default_value_t = false)] no_cache: bool,
    },
    /// Mock hiển thị log gần nhất
    Logs { #[arg(long)] app: Option<String> },
    /// Mock liệt kê ứng dụng
    List {},
    /// Sinh shell completions (ẩn)
    #[command(hide = true)]
    Completions { #[arg(long, default_value = "bash")] shell: String },
    /// Simulate network error (hidden, for testing exit codes)
    #[command(hide = true)]
    Netfail {},
    /// Simulate IO error (hidden, for testing exit codes)
    #[command(hide = true)]
    Iofail {},
    /// Simulate usage error (hidden, for testing exit codes)
    #[command(hide = true)]
    Usagefail {},
    /// Simulate runtime error (hidden, for testing exit codes)
    #[command(hide = true)]
    Runtimefail {},
}
