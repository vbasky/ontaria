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
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/simple.ttl");
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
