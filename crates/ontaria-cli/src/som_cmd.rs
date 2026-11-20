//! Story Object Model commands. The `som` crate does the work.

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};
use som::Family as SomFamily;
use som::model::{Payload, decode_message};
use som::validate::{Outcome, Validator};

use crate::cli::SomCommand;
use crate::output::{print_error, print_success};

pub fn run(
    command: SomCommand,
    offline: bool,
    cache_dir: Option<&Path>,
) -> ontaria::Result<ExitCode> {
    match command {
        SomCommand::Validate { message } => check(&message, false, offline, cache_dir),
        SomCommand::Decode { message } => check(&message, true, offline, cache_dir),
    }
}

fn check(
    spec: &Path,
    decode: bool,
    offline: bool,
    cache_dir: Option<&Path>,
) -> ontaria::Result<ExitCode> {
    let bytes = read_input(spec, offline, cache_dir)?;
    let message: Value = serde_json::from_slice(&bytes)
        .map_err(|err| ontaria::Error::message(format!("failed to parse JSON: {err}")))?;
    let validator = Validator::shared().map_err(|err| ontaria::Error::message(err.to_string()))?;
    let spec_text = spec.to_string_lossy();
    if message.get("payload").is_some() {
        return check_envelope(validator, message, decode);
    }
    check_payload(validator, &message, &spec_text, decode)
}

fn check_envelope(
    validator: &Validator,
    message: Value,
    decode: bool,
) -> ontaria::Result<ExitCode> {
    let outcome = match validator.validate_message(&message) {
        Ok(outcome) => outcome,
        Err(rejection) => {
            print_error(rejection.to_string())?;
            return Ok(ExitCode::from(1));
        }
    };
    let (family, message_type) = match outcome {
        Outcome::Accepted {
            family,
            message_type,
        } => (Some(family.to_string()), Some(message_type)),
        Outcome::Ignored { message_type } => (None, Some(message_type)),
        _ => {
            return Err(ontaria::Error::message(
                "unrecognized SOM validation outcome",
            ));
        }
    };
    if !decode {
        print_success(report(&family, &message_type, None), &[])?;
        return Ok(ExitCode::SUCCESS);
    }
    let decoded =
        decode_message(message).map_err(|err| ontaria::Error::message(err.to_string()))?;
    if family.is_some() && decoded.is_none() {
        return Err(ontaria::Error::message(
            "SOM accepted the message and then decoded no payload",
        ));
    }
    let decoded = decoded
        .map(serde_json::to_value)
        .transpose()
        .map_err(|err| {
            ontaria::Error::message(format!("failed to encode decoded message: {err}"))
        })?;
    print_success(report(&family, &message_type, decoded), &[])?;
    Ok(ExitCode::SUCCESS)
}

fn check_payload(
    validator: &Validator,
    message: &Value,
    spec: &str,
    decode: bool,
) -> ontaria::Result<ExitCode> {
    let family = family_of(message, spec).ok_or_else(|| {
        ontaria::Error::message(
            "SOM payload has no message_type, and the URL or path does not name a family",
        )
    })?;
    if let Err(rejection) = validator.validate_payload(family, message) {
        print_error(rejection.to_string())?;
        return Ok(ExitCode::from(1));
    }
    let family_name = Some(family.to_string());
    let message_type = message
        .get("message_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    if !decode {
        print_success(report(&family_name, &message_type, None), &[])?;
        return Ok(ExitCode::SUCCESS);
    }
    let decoded = Payload::decode(family, message.clone())
        .map_err(|err| ontaria::Error::message(err.to_string()))?;
    let decoded = serde_json::to_value(decoded).map_err(|err| {
        ontaria::Error::message(format!("failed to encode decoded payload: {err}"))
    })?;
    print_success(report(&family_name, &message_type, Some(decoded)), &[])?;
    Ok(ExitCode::SUCCESS)
}

fn report(family: &Option<String>, message_type: &Option<String>, message: Option<Value>) -> Value {
    let mut value = json!({
        "outcome": if family.is_some() { "accepted" } else { "ignored" },
        "family": family,
        "message_type": message_type,
    });
    if let Some(message) = message {
        value["message"] = message;
    }
    value
}

fn family_of(message: &Value, spec: &str) -> Option<SomFamily> {
    let named = message
        .get("message_type")
        .and_then(Value::as_str)
        .and_then(SomFamily::from_message_type);
    named.or_else(|| family_from_spec(spec))
}

fn family_from_spec(spec: &str) -> Option<SomFamily> {
    const DIRS: &[(&str, SomFamily)] = &[
        ("hurricane-run", SomFamily::StoryContext),
        ("story-context", SomFamily::StoryContext),
        ("telling", SomFamily::Telling),
        ("delivery", SomFamily::DeliveryMediaAvailable),
        ("link", SomFamily::Link),
        ("system-audit", SomFamily::SystemAudit),
        ("skill-warning", SomFamily::SkillWarning),
    ];
    spec.split(['/', '\\']).find_map(|part| {
        DIRS.iter()
            .find(|(name, _)| *name == part)
            .map(|(_, family)| *family)
    })
}

fn read_input(spec: &Path, offline: bool, cache_dir: Option<&Path>) -> ontaria::Result<Vec<u8>> {
    let text = spec.to_string_lossy();
    if ontaria::is_remote(&text) {
        if offline {
            return Err(ontaria::Error::message(
                "a SOM URL needs the network; omit --offline",
            ));
        }
        let cache = match cache_dir {
            Some(dir) => ontaria::CacheManager::open(dir)?,
            None => ontaria::CacheManager::shared()?,
        };
        cache.fetch(&text)?;
        return cache.load(&text);
    }
    std::fs::read(spec).map_err(|source| ontaria::Error::Read {
        path: spec.to_path_buf(),
        source,
    })
}
