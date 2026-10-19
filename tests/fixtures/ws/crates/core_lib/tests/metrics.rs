#[test]
fn metrics_say_where() {
    assert!(core_lib::serve_metrics("x").contains("x"));
}
