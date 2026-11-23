use criterion::{Criterion, criterion_group, criterion_main};

fn parse_simple(c: &mut Criterion) {
    let bytes = include_bytes!("../../../testdata/simple.ttl");
    c.bench_function("parse simple.ttl", |bencher| {
        bencher.iter(|| ontaria::parse_document(bytes, "http://example.org/").unwrap());
    });
}

criterion_group!(benches, parse_simple);
criterion_main!(benches);
