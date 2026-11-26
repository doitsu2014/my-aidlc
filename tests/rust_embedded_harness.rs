use dtech_aidlc::scope::embedded_asset;

#[test]
fn bundled_harness_manifests_reference_embedded_files() {
    for harness in ["claude", "deepseek"] {
        let path = format!("harness/{harness}/manifest.json");
        let manifest: serde_json::Value =
            serde_json::from_slice(embedded_asset(&path).expect("manifest embedded")).unwrap();
        assert_eq!(manifest["name"], harness);
        for file in manifest["files"].as_array().unwrap() {
            let source = file["source"].as_str().unwrap();
            assert!(
                embedded_asset(&format!("harness/{harness}/{source}")).is_some(),
                "missing {source}"
            );
        }
    }
    assert!(embedded_asset("data/scaffold/PROJECT.md").is_some());
}
