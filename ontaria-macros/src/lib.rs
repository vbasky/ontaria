//! `terms!("ontology.ttl")` expands to constants and a class enum.
//!
//! The path is relative to the Cargo manifest of the crate that invokes the
//! macro. A misspelt variant then fails at compile time because it is not a
//! member of the generated enum. This crate does not depend on `ontaria`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oxttl::TurtleParser;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Ident, LitStr, parse_macro_input};

/// Load `path` at compile time and emit `classes`, `properties`, and `Class`.
#[proc_macro]
pub fn terms(input: TokenStream) -> TokenStream {
    let path = parse_macro_input!(input as LitStr);
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full = PathBuf::from(manifest).join(path.value());
    let bytes = std::fs::read(&full).unwrap_or_else(|err| {
        panic!("ontaria terms!: failed to read {}: {err}", full.display());
    });
    let (classes, properties) = parse(&bytes, "http://ontaria.local/terms");
    emit(&classes, &properties)
}

fn parse(bytes: &[u8], base: &str) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut parser = TurtleParser::new()
        .with_base_iri(base)
        .expect("base iri")
        .for_slice(bytes);
    let mut classes = BTreeMap::new();
    let mut properties = BTreeMap::new();
    for triple in &mut parser {
        let triple = triple.unwrap_or_else(|err| panic!("ontaria terms!: {err}"));
        let oxrdf::NamedOrBlankNode::NamedNode(subject) = &triple.subject else {
            continue;
        };
        let oxrdf::Term::NamedNode(object) = &triple.object else {
            continue;
        };
        if triple.predicate.as_str() != "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
            continue;
        }
        let iri = subject.as_str().to_string();
        let name = local_name(&iri);
        match object.as_str() {
            "http://www.w3.org/2002/07/owl#Class"
            | "http://www.w3.org/2000/01/rdf-schema#Class" => {
                classes.insert(iri, name);
            }
            "http://www.w3.org/2002/07/owl#ObjectProperty"
            | "http://www.w3.org/2002/07/owl#DatatypeProperty"
            | "http://www.w3.org/2002/07/owl#AnnotationProperty"
            | "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property" => {
                properties.insert(iri, name);
            }
            _ => {}
        }
    }
    (classes, properties)
}

fn emit(classes: &BTreeMap<String, String>, properties: &BTreeMap<String, String>) -> TokenStream {
    let class_consts = named(classes);
    let property_consts = named(properties);
    let class_const_name: Vec<_> = class_consts
        .iter()
        .map(|(_, name)| Ident::new(name, Span::call_site()))
        .collect();
    let class_const_iri: Vec<_> = class_consts.iter().map(|(iri, _)| iri.clone()).collect();
    let property_const_name: Vec<_> = property_consts
        .iter()
        .map(|(_, name)| Ident::new(name, Span::call_site()))
        .collect();
    let property_const_iri: Vec<_> = property_consts.iter().map(|(iri, _)| iri.clone()).collect();
    let variants: Vec<_> = class_consts
        .iter()
        .map(|(_, name)| Ident::new(&pascal(name), Span::call_site()))
        .collect();
    let variant_consts = class_const_name.clone();
    let classes_mod = Ident::new("classes", Span::call_site());
    let properties_mod = Ident::new("properties", Span::call_site());
    let class_enum = Ident::new("Class", Span::call_site());
    let enum_tokens = if variants.is_empty() {
        quote! {}
    } else {
        quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum #class_enum {
                #(#variants,)*
            }
            impl #class_enum {
                pub fn iri(self) -> &'static str {
                    match self {
                        #(Self::#variants => #classes_mod::#variant_consts,)*
                    }
                }
            }
        }
    };
    quote! {
        #[allow(dead_code)]
        pub mod #classes_mod {
            #(pub const #class_const_name: &str = #class_const_iri;)*
        }
        #[allow(dead_code)]
        pub mod #properties_mod {
            #(pub const #property_const_name: &str = #property_const_iri;)*
        }
        #enum_tokens
    }
    .into()
}

fn named(items: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut used = BTreeMap::<String, usize>::new();
    let mut named = Vec::new();
    for (iri, local) in items {
        let base = shouty(local);
        let count = used.entry(base.clone()).or_insert(0);
        *count += 1;
        let name = if *count == 1 {
            base
        } else {
            format!("{base}_{count}")
        };
        named.push((iri.clone(), name));
    }
    named
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
}

fn shouty(local: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in local.chars() {
        if ch.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            prev_lower = ch.is_ascii_lowercase();
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
            prev_lower = false;
        }
    }
    let out = out.trim_matches('_');
    let out = if out.is_empty() { "TERM" } else { out };
    if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("N_{out}")
    } else {
        out.to_string()
    }
}

fn pascal(shouty_name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in shouty_name.chars() {
        if ch == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}
