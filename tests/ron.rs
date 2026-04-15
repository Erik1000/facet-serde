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
struct WithBoxed {
    value: Box<u32>,
}

// ── Helper ──────────────────────────────────────────────────────────────

fn ron_roundtrip<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    let adapter = Adapter::new(value.clone());
    let ron_str = ron::to_string(&adapter).expect("ron serialize");
    let back: Adapter<T> = ron::from_str(&ron_str)
        .unwrap_or_else(|e| panic!("ron deserialize failed: {e}\nron: {ron_str}"));
    assert_eq!(
        back.into_inner(),
        value,
        "ron roundtrip failed for: {ron_str}"
    );
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn ron_primitives() {
    ron_roundtrip(42u32);
    ron_roundtrip(0u8);
    ron_roundtrip(255u8);
    ron_roundtrip(-1i8);
    ron_roundtrip(true);
    ron_roundtrip(false);
    ron_roundtrip(core::f64::consts::PI);
    ron_roundtrip(0.0f32);
    ron_roundtrip("hello".to_string());
    ron_roundtrip(String::new());
    ron_roundtrip('a');
}

#[test]
fn ron_simple_struct() {
    ron_roundtrip(Simple {
        name: "Alice".to_string(),
        age: 30,
        active: true,
    });
}

#[test]
fn ron_nested_struct() {
    ron_roundtrip(Nested {
        label: "origin".to_string(),
        point: Point { x: 1.5, y: -2.5 },
    });
}

#[test]
fn ron_option() {
    ron_roundtrip(WithOption {
        value: Some("present".to_string()),
    });
    ron_roundtrip(WithOption { value: None });
}

#[test]
fn ron_vec() {
    ron_roundtrip(WithVec {
        items: vec![10, 20, 30],
    });
    ron_roundtrip(WithVec { items: vec![] });
}

#[test]
fn ron_hashmap() {
    let mut data = HashMap::new();
    data.insert("x".to_string(), 100);
    ron_roundtrip(WithMap { data });
    ron_roundtrip(WithMap {
        data: HashMap::new(),
    });
}

#[test]
fn ron_unit_enum() {
    ron_roundtrip(Color::Red);
    ron_roundtrip(Color::Green);
    ron_roundtrip(Color::Blue);
}

#[test]
fn ron_complex_enum() {
    ron_roundtrip(Shape::Circle(2.5));
    ron_roundtrip(Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    });
    ron_roundtrip(Shape::Point);
}

#[test]
fn ron_boxed() {
    ron_roundtrip(WithBoxed {
        value: Box::new(42),
    });
}
