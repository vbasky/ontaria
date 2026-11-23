use ontaria_macros::terms;

terms!("fixtures/mini.ttl");

#[test]
fn person_constant_matches_the_iri() {
    assert_eq!(classes::PERSON, "http://example.org/Person");
    assert_eq!(properties::NAME, "http://example.org/name");
    assert_eq!(Class::Person.iri(), classes::PERSON);
}
