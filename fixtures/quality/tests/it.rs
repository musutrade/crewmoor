fn number() -> u8 {
    "1".parse::<u8>().unwrap()
}

#[test]
fn external_sources_and_test_helper() {
    assert_eq!(quality_fixture::external::external(true), number());
    assert_eq!(quality_fixture::external::external(false), 0);
    assert_eq!(quality_fixture::included(), number());
}
