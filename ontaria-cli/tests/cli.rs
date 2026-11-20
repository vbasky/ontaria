use std::process::Command;

use serde_json::Value;

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ontaria"))
}

#[test]
fn help_lists_the_query_commands() {
    let output = tool().arg("--help").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for command in [
        "fetch",
        "get-class",
        "get-property",
        "list-class-properties",
        "batch-get-class",
        "batch-search",
    ] {
        assert!(text.contains(command), "help is missing {command}: {text}");
    }
}

#[test]
fn get_class_reads_a_local_turtle_file() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/simple.ttl");
    let output = tool()
        .args(["get-class", "ex:Person", "--inherited", "-o", fixture])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["class"]["label"], "Person");
    let inherited = value["data"]["inherited_properties"].as_array().unwrap();
    assert_eq!(inherited[0]["property"]["label"], "name");
}

#[test]
fn missing_cached_ontology_is_a_json_error() {
    let output = tool()
        .args([
            "get-class",
            "Person",
            "-o",
            "https://example.invalid/not-cached.ttl",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "error");
    assert!(value["message"].as_str().unwrap().contains("not cached"));
}

#[test]
fn validate_reads_turtle_from_a_filesystem_path() {
    let ontology = concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/news.ttl");
    let data = std::env::temp_dir().join(format!("ontaria-validate-{}.ttl", std::process::id()));
    std::fs::write(
        &data,
        r#"
@prefix news: <http://example.org/news#> .
news:story a news:NewsItem ;
    news:headline "Hello" .
"#,
    )
    .unwrap();
    let output = tool()
        .args(["-o", ontology, "validate", &data.to_string_lossy()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&data);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert!(value["data"]["issues"].as_array().unwrap().is_empty());
}

#[test]
fn som_validate_accepts_the_upstream_hurricane_envelope() {
    let message = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../third_party/som/examples/story-context/hurricane-beat6-envelope.json"
    );
    let output = tool().args(["som", "validate", message]).output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["outcome"], "accepted");
    assert_eq!(value["data"]["message_type"], "story.context");
}

#[test]
fn som_validate_rejects_a_pre_1_message() {
    let path = std::env::temp_dir().join(format!("ontaria-som-{}.json", std::process::id()));
    std::fs::write(
        &path,
        r#"{"som_version":"0.3.2","message_id":"0199a1c4-7a2e-7b31-8c55-4d2f9e6a1b07","correlation_id":"0199a1c4-0000-7000-8000-000000000911","message_type":"story.context","timestamp":"2026-09-11T16:41:00.000000Z","originating_system":{"system_id":"ncs-nyc-01","system_type":"ncs"},"topic":"som.story.context.hurricane-2026-0911","payload":{}}"#,
    )
    .unwrap();
    let output = tool()
        .args(["som", "validate", &path.to_string_lossy()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "error");
    assert!(value["message"].as_str().unwrap().contains("not SOM 1.x"));
}

#[test]
fn som_validate_reads_published_example_urls() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../third_party/som/examples");
    let cache = std::env::temp_dir().join(format!("ontaria-som-url-{}", std::process::id()));
    let dirs = [
        "hurricane-run",
        "story-context",
        "telling",
        "delivery",
        "link",
        "system-audit",
        "skill-warning",
    ];
    for dir in dirs {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy();
            let url = format!(
                "https://raw.githubusercontent.com/storyobjectmodel/som/ff1e94af44d9a4d0c7444dc2294eeffc77b0876b/examples/{dir}/{name}"
            );
            let output = tool()
                .args([
                    "--cache-dir",
                    &cache.to_string_lossy(),
                    "som",
                    "validate",
                    &url,
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{url}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["status"], "ok", "{url}");
            assert_eq!(value["data"]["outcome"], "accepted", "{url}");
        }
    }
    let envelope = "https://raw.githubusercontent.com/storyobjectmodel/som/ff1e94af44d9a4d0c7444dc2294eeffc77b0876b/examples/story-context/hurricane-beat6-envelope.json";
    let output = tool()
        .args([
            "--cache-dir",
            &cache.to_string_lossy(),
            "som",
            "decode",
            envelope,
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&cache);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["data"]["message"]["payload"]["story_id"],
        "hurricane-2026-0911"
    );
}
