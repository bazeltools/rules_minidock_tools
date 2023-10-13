use clap::Parser;
use rules_minidock_tools::container_specs::ConfigDelta;
use serde_json;
use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_merge_app() {
    let dir = tempdir().expect("Failed to create temp dir");
    let dir_path = dir.path().to_str().expect("Failed to convert path to str");

    let config_path = format!("{}/config.json", dir_path);
    let manifest_path = format!("{}/manifest.json", dir_path);
    let manifest_sha256_path = format!("{}/manifest_sha256.json", dir_path);
    let upload_metadata_path = format!("{}/upload_metadata.json", dir_path);

    let args = vec![
        "merge app",
        "--merger-config-path",
        "tests/data/test_assemble_simple_merger_config_file.json",
        "--config-path",
        &config_path,
        "--manifest-path",
        &manifest_path,
        "--manifest-sha256-path",
        &manifest_sha256_path,
        "--upload-metadata-path",
        &upload_metadata_path,
    ];
    let mut args_with_ext = args.clone();

    let opt = Parser::try_parse_from(args).unwrap();
    let result = rules_minidock_tools::merge_main(opt).await;
    assert!(result.is_ok());

    let json_str = fs::read_to_string(&config_path).unwrap();
    let config_delta: ConfigDelta = serde_json::from_str(&json_str).unwrap();
    let mut expected_labels = BTreeMap::new();
    expected_labels.insert("label1".to_string(), "foo".to_string());
    expected_labels.insert("label2".to_string(), "bar".to_string());
    assert!(config_delta.config.unwrap().labels.unwrap() == expected_labels);

    // When we give it an external config path, we should see those Labels show up
    args_with_ext.push("--external-config-path");
    args_with_ext.push("tests/data/external_merge_config.json");

    let opt_with_ext = Parser::try_parse_from(args_with_ext).unwrap();
    let result_with_ext = rules_minidock_tools::merge_main(opt_with_ext).await;
    assert!(result_with_ext.is_ok());

    let json_str_2 = fs::read_to_string(&config_path).unwrap();
    let config_delta_2: ConfigDelta = serde_json::from_str(&json_str_2).unwrap();
    let mut expected_labels_2 = BTreeMap::new();
    expected_labels_2.insert(
        "external-config-label-1".to_string(),
        "extlabel1".to_string(),
    );
    expected_labels_2.insert(
        "external-config-label-2".to_string(),
        "extlabel2".to_string(),
    );
    // Notably, we use label1 from the rules themselves, not from the external merge config
    expected_labels_2.insert("label1".to_string(), "foo".to_string());
    expected_labels_2.insert("label2".to_string(), "bar".to_string());
    let config = config_delta_2.config.unwrap();
    assert!(config.labels.unwrap() == expected_labels_2);
    // Ensure the Env is here too
    assert!(config
        .env
        .unwrap()
        .contains(&"EXTERNALENV1=extenv1".to_string()));
}
