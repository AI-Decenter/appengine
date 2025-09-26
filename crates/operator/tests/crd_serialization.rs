use aether_operator::AetherAppSpec;
use serde_json::json;

#[test]
fn crd_struct_roundtrip() {
    let original = AetherAppSpec { image: "example:image".into(), replicas: Some(2) };
    let j = serde_json::to_value(&original).unwrap();
    assert_eq!(j, json!({"image":"example:image","replicas":2}));
    let back: AetherAppSpec = serde_json::from_value(j).unwrap();
    assert_eq!(back, original);
}
