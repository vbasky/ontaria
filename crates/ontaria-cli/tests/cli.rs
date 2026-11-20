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
        "ontology lookup",
        "--log-file",
        "--ontology",
        "Input:",
        "Output:",
        "Ontology:",
    ] {
        assert!(text.contains(command), "help is missing {command}: {text}");
    }
}

#[test]
fn no_args_prints_help() {
    let output = tool().output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("get-class"), "{text}");
    assert!(text.contains("ontology lookup"), "{text}");
}

#[test]
fn positional_path_summarizes_a_turtle_file() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/simple.ttl");
    let output = tool().arg(fixture).output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["format"], "turtle");
    assert!(value["data"]["classes"].as_u64().unwrap() >= 2);
    assert!(
        value["data"]["path"]
            .as_str()
            .unwrap()
            .ends_with("simple.ttl")
    );
}

#[test]
fn positional_path_can_stand_in_for_ontology_flag() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/simple.ttl");
    let output = tool()
        .args([fixture, "get-class", "ex:Person", "--inherited"])
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
}

#[test]
fn several_paths_return_an_array() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let output = tool()
        .args([&format!("{root}/simple.ttl"), &format!("{root}/news.ttl")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"].as_array().unwrap().len(), 2);
}

#[test]
fn log_file_receives_the_envelope() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/simple.ttl");
    let log = std::env::temp_dir().join(format!("ontaria-log-{}.json", std::process::id()));
    let output = tool()
        .args(["--log-file", &log.to_string_lossy(), fixture])
        .output()
        .unwrap();
    let bytes = std::fs::read(&log).unwrap();
    let _ = std::fs::remove_file(&log);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["format"], "turtle");
}

fn help_text(args: &[&str]) -> String {
    let output = tool().args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn flags_follow_the_command_that_uses_them() {
    let class_help = help_text(&["get-class", "--help"]);
    assert!(class_help.contains("--ontology"));
    assert!(class_help.contains("--imports"));
    assert_eq!(class_help.matches("--ontology").count(), 1);

    let diff_help = help_text(&["diff", "--help"]);
    assert!(!diff_help.contains("--ontology"), "{diff_help}");
    assert!(!diff_help.contains("--imports"), "{diff_help}");
    assert!(!diff_help.contains("--cache-dir"), "{diff_help}");

    let som_help = help_text(&["som", "validate", "--help"]);
    assert!(!som_help.contains("--ontology"), "{som_help}");
    assert!(!som_help.contains("--imports"), "{som_help}");
    assert!(som_help.contains("--cache-dir"), "{som_help}");
    assert!(som_help.contains("--offline"), "{som_help}");

    let fetch_help = help_text(&["fetch", "--help"]);
    assert!(!fetch_help.contains("--ontology"), "{fetch_help}");
    assert!(fetch_help.contains("--cache-dir"), "{fetch_help}");

    let map_apply = help_text(&["map", "apply", "--help"]);
    assert!(!map_apply.contains("--ontology"), "{map_apply}");
    let map_validate = help_text(&["map", "validate", "--help"]);
    assert!(map_validate.contains("--ontology"), "{map_validate}");
    let gen_context = help_text(&["gen", "context", "--help"]);
    assert!(gen_context.contains("--ontology"), "{gen_context}");

    let root_help = help_text(&["--help"]);
    assert!(!root_help.contains("list-cached"), "{root_help}");
    assert!(root_help.contains("Ontology:"));
    assert!(root_help.contains("Input:"));
}

#[test]
fn diff_rejects_a_leading_ontology_flag() {
    let output = tool()
        .args(["-o", "a.ttl", "diff", "b.ttl", "c.ttl"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "error");
    assert!(
        value["message"]
            .as_str()
            .unwrap()
            .contains("does not take ontology options")
    );
}

#[test]
fn the_same_path_and_ontology_flag_name_one_file() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/simple.ttl");
    let output = tool()
        .args([
            fixture,
            "get-class",
            "ex:Person",
            "--inherited",
            "-o",
            fixture,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["class"]["label"], "Person");
}

#[test]
fn list_cached_remains_available() {
    let cache = std::env::temp_dir().join(format!("ontaria-list-{}", std::process::id()));
    let output = tool()
        .args(["--cache-dir", &cache.to_string_lossy(), "list-cached"])
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
    assert_eq!(value["status"], "ok");
}

#[test]
fn a_subcommand_rejects_two_ontology_paths() {
    let output = tool()
        .args(["a.ttl", "b.ttl", "get-class", "Person"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "error");
    assert!(
        value["message"]
            .as_str()
            .unwrap()
            .contains("one ontology PATH")
    );
}

#[test]
fn get_class_reads_a_local_turtle_file() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/simple.ttl");
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
    let ontology = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/news.ttl");
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
        "/../../third_party/som/examples/story-context/hurricane-beat6-envelope.json"
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
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/som/examples");
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
