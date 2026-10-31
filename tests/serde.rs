#[path = "../examples/serde_formats.rs"]
mod example;

#[test]
fn multiple_registered_formats_round_trip_through_serde_types() {
    example::run().unwrap();
}
