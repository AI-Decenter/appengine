use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[kube(group = "aether.dev", version = "v1", kind = "AetherApp", namespaced, status = "AetherAppStatus")]
pub struct AetherAppSpec {
    pub image: String,
    #[serde(default)]
    pub replicas: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema, PartialEq)]
pub struct AetherAppStatus {
    pub observed_generation: Option<i64>,
    pub last_reconcile: Option<String>,
}

// Re-export commonly used symbols for convenience in binaries/tests.
pub use AetherAppSpec as Spec;
pub use AetherAppStatus as Status;