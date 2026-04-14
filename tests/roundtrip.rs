use facet::Facet;
use facet_serde::Adapter;
use std::collections::HashMap;

// ── Test types ──────────────────────────────────────────────────────────

#[derive(Facet, Debug, PartialEq, Clone)]
struct Simple {
    name: String,
    age: u32,
    active: bool,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct Nested {
    label: String,
    point: Point,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithOption {
    value: Option<String>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithVec {
    items: Vec<u32>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithMap {
    data: HashMap<String, i32>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
enum Shape {
    Circle(f64),
    Rectangle { width: f64, height: f64 },
    Point,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithNestedOption {
    a: Option<Option<i32>>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithBoxed {
    value: Box<u32>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct WithTuple {
    pair: (i32, String),
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct Empty;

// ── Helpers ─────────────────────────────────────────────────────────────

/// Serialize with facet-serde (via serde_json), deserialize back, and assert equality.
fn serde_roundtrip<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    let adapter = Adapter::new(value.clone());
    let json = serde_json::to_string(&adapter).expect("serde_json serialize");
    let back: Adapter<T> = serde_json::from_str(&json).expect("serde_json deserialize");
    assert_eq!(
        back.into_inner(),
        value,
        "serde roundtrip failed for json: {json}"
    );
}

/// Serialize with facet-json, deserialize with facet-serde (via serde_json).
fn facet_json_to_serde<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    let json = facet_json::to_string(&value).expect("facet_json serialize");
    let back: Adapter<T> = serde_json::from_str(&json).unwrap_or_else(|e| {
        panic!("serde_json deserialize of facet-json output failed: {e}\njson: {json}")
    });
    assert_eq!(
        back.into_inner(),
        value,
        "facet-json → serde roundtrip failed for json: {json}"
    );
}

/// Serialize with facet-serde (via serde_json), deserialize with facet-json.
fn serde_to_facet_json<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    let json = serde_json::to_string(&Adapter::new(value.clone())).expect("serde_json serialize");
    let back: T = facet_json::from_str(&json).unwrap_or_else(|e| {
        panic!("facet_json deserialize of serde_json output failed: {e}\njson: {json}")
    });
    assert_eq!(
        back, value,
        "serde → facet-json roundtrip failed for json: {json}"
    );
}

/// Full cross-format roundtrip: both directions.
fn cross_roundtrip<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    serde_roundtrip(value.clone());
    facet_json_to_serde(value.clone());
    serde_to_facet_json(value);
}

// ── Serde-only roundtrip tests ──────────────────────────────────────────

#[test]
fn serde_primitives() {
    serde_roundtrip(42u32);
    serde_roundtrip(0u8);
    serde_roundtrip(255u8);
    serde_roundtrip(-1i8);
    serde_roundtrip(true);
    serde_roundtrip(false);
    serde_roundtrip(core::f64::consts::PI);
    serde_roundtrip(0.0f32);
    serde_roundtrip("hello".to_string());
    serde_roundtrip(String::new());
    serde_roundtrip('a');
}

#[test]
fn serde_simple_struct() {
    serde_roundtrip(Simple {
        name: "Alice".to_string(),
        age: 30,
        active: true,
    });
}

#[test]
fn serde_nested_struct() {
    serde_roundtrip(Nested {
        label: "origin".to_string(),
        point: Point { x: 1.5, y: -2.5 },
    });
}

#[test]
fn serde_option() {
    serde_roundtrip(WithOption {
        value: Some("present".to_string()),
    });
    serde_roundtrip(WithOption { value: None });
}

#[test]
fn serde_vec() {
    serde_roundtrip(WithVec {
        items: vec![10, 20, 30],
    });
    serde_roundtrip(WithVec { items: vec![] });
}

#[test]
fn serde_hashmap() {
    let mut data = HashMap::new();
    data.insert("x".to_string(), 100);
    data.insert("y".to_string(), -50);
    serde_roundtrip(WithMap { data });
    serde_roundtrip(WithMap {
        data: HashMap::new(),
    });
}

#[test]
fn serde_unit_enum() {
    serde_roundtrip(Color::Red);
    serde_roundtrip(Color::Green);
    serde_roundtrip(Color::Blue);
}

#[test]
fn serde_complex_enum() {
    serde_roundtrip(Shape::Circle(2.5));
    serde_roundtrip(Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    });
    serde_roundtrip(Shape::Point);
}

#[test]
fn serde_boxed() {
    serde_roundtrip(WithBoxed {
        value: Box::new(42),
    });
}

// ── Cross-format roundtrip tests (facet-json ↔ serde_json) ─────────────

#[test]
fn cross_primitives() {
    cross_roundtrip(42u32);
    cross_roundtrip(0u8);
    cross_roundtrip(true);
    cross_roundtrip(core::f64::consts::PI);
    cross_roundtrip("hello world".to_string());
}

#[test]
fn cross_simple_struct() {
    cross_roundtrip(Simple {
        name: "Bob".to_string(),
        age: 25,
        active: false,
    });
}

#[test]
fn cross_nested_struct() {
    cross_roundtrip(Nested {
        label: "test".to_string(),
        point: Point { x: 3.0, y: 4.0 },
    });
}

#[test]
fn cross_option() {
    cross_roundtrip(WithOption {
        value: Some("data".to_string()),
    });
    cross_roundtrip(WithOption { value: None });
}

#[test]
fn cross_vec() {
    cross_roundtrip(WithVec {
        items: vec![1, 2, 3, 4, 5],
    });
    cross_roundtrip(WithVec { items: vec![] });
}

#[test]
fn cross_hashmap() {
    // Use a single-entry map to avoid ordering issues in comparison.
    let mut data = HashMap::new();
    data.insert("key".to_string(), 99);
    cross_roundtrip(WithMap { data });
}

#[test]
fn cross_unit_enum() {
    cross_roundtrip(Color::Red);
    cross_roundtrip(Color::Green);
    cross_roundtrip(Color::Blue);
}

#[test]
fn cross_complex_enum() {
    cross_roundtrip(Shape::Circle(7.0));
    cross_roundtrip(Shape::Point);
}

#[test]
fn cross_boxed() {
    cross_roundtrip(WithBoxed {
        value: Box::new(99),
    });
}

// ── JSON output compatibility tests ─────────────────────────────────────

#[test]
fn facet_json_output_matches_serde_for_struct() {
    let value = Simple {
        name: "Eve".to_string(),
        age: 40,
        active: true,
    };

    let serde_json_str = serde_json::to_string(&Adapter::new(value.clone())).unwrap();
    let facet_json_str = facet_json::to_string(&value).unwrap();

    // Parse both as serde_json::Value so field order doesn't matter.
    let serde_val: serde_json::Value = serde_json::from_str(&serde_json_str).unwrap();
    let facet_val: serde_json::Value = serde_json::from_str(&facet_json_str).unwrap();
    assert_eq!(
        serde_val, facet_val,
        "JSON output mismatch:\n  serde: {serde_json_str}\n  facet: {facet_json_str}"
    );
}

#[test]
fn facet_json_output_matches_serde_for_vec() {
    let value = WithVec {
        items: vec![10, 20, 30],
    };

    let serde_json_str = serde_json::to_string(&Adapter::new(value.clone())).unwrap();
    let facet_json_str = facet_json::to_string(&value).unwrap();

    let serde_val: serde_json::Value = serde_json::from_str(&serde_json_str).unwrap();
    let facet_val: serde_json::Value = serde_json::from_str(&facet_json_str).unwrap();
    assert_eq!(serde_val, facet_val);
}

#[test]
fn facet_json_output_matches_serde_for_option_some() {
    let value = WithOption {
        value: Some("test".to_string()),
    };

    let serde_json_str = serde_json::to_string(&Adapter::new(value.clone())).unwrap();
    let facet_json_str = facet_json::to_string(&value).unwrap();

    let serde_val: serde_json::Value = serde_json::from_str(&serde_json_str).unwrap();
    let facet_val: serde_json::Value = serde_json::from_str(&facet_json_str).unwrap();
    assert_eq!(serde_val, facet_val);
}

#[test]
fn facet_json_output_matches_serde_for_option_none() {
    let value = WithOption { value: None };

    let serde_json_str = serde_json::to_string(&Adapter::new(value.clone())).unwrap();
    let facet_json_str = facet_json::to_string(&value).unwrap();

    let serde_val: serde_json::Value = serde_json::from_str(&serde_json_str).unwrap();
    let facet_val: serde_json::Value = serde_json::from_str(&facet_json_str).unwrap();
    assert_eq!(serde_val, facet_val);
}

#[test]
fn facet_json_output_matches_serde_for_nested() {
    let value = Nested {
        label: "nested".to_string(),
        point: Point { x: 1.5, y: 2.5 },
    };

    let serde_json_str = serde_json::to_string(&Adapter::new(value.clone())).unwrap();
    let facet_json_str = facet_json::to_string(&value).unwrap();

    let serde_val: serde_json::Value = serde_json::from_str(&serde_json_str).unwrap();
    let facet_val: serde_json::Value = serde_json::from_str(&facet_json_str).unwrap();
    assert_eq!(serde_val, facet_val);
}
