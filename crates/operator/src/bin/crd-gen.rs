use kube::CustomResourceExt;
use aether_operator::AetherApp; // bring in CRD type
fn main() {
    let crd = AetherApp::crd();
    let yaml = serde_yaml::to_string(&crd).expect("serialize CRD");
    println!("{}", yaml);
}
