//! Read-only HTTP API over one loaded ontology.
//!
//! Routes are GET `/class`, `/property`, `/search`, `/parents`, `/children`,
//! and `/health`. The process prints nothing on stdout except errors from
//! the caller. Request logs go to stderr.

use std::io::Cursor;

use ontaria::{Ontology, Warning};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::output::print_success;

pub fn serve(ontology: Ontology, bind: &str) -> ontaria::Result<std::process::ExitCode> {
    let server = Server::http(bind)
        .map_err(|err| ontaria::Error::message(format!("failed to bind {bind}: {err}")))?;
    eprintln!("ontaria listening on http://{bind}");
    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
        let body = match path {
            "/health" => ok(serde_json::json!({"status": "ok"})),
            "/class" => lookup_class(&ontology, query),
            "/property" => lookup_property(&ontology, query),
            "/parents" => lookup_parents(&ontology, query),
            "/children" => lookup_children(&ontology, query),
            "/search" => lookup_search(&ontology, query),
            _ => json_response(
                StatusCode(404),
                serde_json::json!({"status": "error", "message": "not found"}),
            ),
        };
        if let Err(err) = request.respond(body) {
            eprintln!("failed to write response: {err}");
        }
    }
    print_success(serde_json::json!({"bind": bind}), &[] as &[Warning])?;
    Ok(std::process::ExitCode::SUCCESS)
}

fn lookup_class(ontology: &Ontology, query: &str) -> Response<Cursor<Vec<u8>>> {
    let Some(term) = query_param(query, "term") else {
        return missing("term");
    };
    match ontology.get_class(&term) {
        Some(class) => ok(class),
        None => not_found("class", &term),
    }
}

fn lookup_property(ontology: &Ontology, query: &str) -> Response<Cursor<Vec<u8>>> {
    let Some(term) = query_param(query, "term") else {
        return missing("term");
    };
    match ontology.get_property(&term) {
        Some(property) => ok(property),
        None => not_found("property", &term),
    }
}

fn lookup_parents(ontology: &Ontology, query: &str) -> Response<Cursor<Vec<u8>>> {
    let Some(term) = query_param(query, "term") else {
        return missing("term");
    };
    match ontology.get_parents(&term) {
        Some(parents) => ok(&parents),
        None => not_found("class", &term),
    }
}

fn lookup_children(ontology: &Ontology, query: &str) -> Response<Cursor<Vec<u8>>> {
    let Some(term) = query_param(query, "term") else {
        return missing("term");
    };
    match ontology.get_children(&term) {
        Some(children) => ok(&children),
        None => not_found("class", &term),
    }
}

fn lookup_search(ontology: &Ontology, query: &str) -> Response<Cursor<Vec<u8>>> {
    let Some(text) = query_param(query, "q") else {
        return missing("q");
    };
    ok(ontology.search(&text))
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return Some(urlencoding_decode(value));
        }
    }
    None
}

fn urlencoding_decode(value: &str) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(' '),
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    index += 3;
                    continue;
                }
                out.push('%');
            }
            byte => out.push(byte as char),
        }
        index += 1;
    }
    out
}

fn ok(data: impl serde::Serialize) -> Response<Cursor<Vec<u8>>> {
    json_response(
        StatusCode(200),
        serde_json::json!({"status": "ok", "data": data, "warnings": []}),
    )
}

fn missing(name: &str) -> Response<Cursor<Vec<u8>>> {
    json_response(
        StatusCode(400),
        serde_json::json!({"status": "error", "message": format!("missing {name}")}),
    )
}

fn not_found(kind: &str, term: &str) -> Response<Cursor<Vec<u8>>> {
    json_response(
        StatusCode(404),
        serde_json::json!({"status": "error", "message": format!("{kind} not found: {term}")}),
    )
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response<Cursor<Vec<u8>>> {
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    Response::from_data(bytes)
        .with_status_code(status)
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("header"),
        )
}
