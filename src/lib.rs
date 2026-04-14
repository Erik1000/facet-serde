#![forbid(unsafe_code)]

mod de;
mod ser;

use facet::Facet;
use facet_reflect::{Partial, Peek};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::de::PartialSeed;
use crate::ser::PeekSerialize;

/// An adapter type that bridges the [`facet`](facet) and [`serde`] ecosystems.
///
/// `Adapter<T>` wraps any type `T` that implements [`Facet`] and provides
/// implementations of [`serde::Serialize`] and [`serde::Deserialize`] by using
/// the underlying facet [`Shape`](facet::Shape) for reflection.
///
/// # Serialization
///
/// Uses [`Peek`] to walk the value's shape tree and translate it into
/// serde `Serializer` calls.
///
/// # Deserialization
///
/// Uses [`Partial`] to build a value from serde `Deserializer` events,
/// materializing the result via [`HeapValue`](facet_reflect::HeapValue).
///
/// # Examples
///
/// ```
/// use facet::Facet;
/// use facet_serde::Adapter;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point { x: f64, y: f64 }
///
/// let point = Point { x: 1.0, y: 2.0 };
/// let json = serde_json::to_string(&Adapter::new(point)).unwrap();
/// let roundtrip: Adapter<Point> = serde_json::from_str(&json).unwrap();
/// assert_eq!(roundtrip.into_inner().x, 1.0);
/// ```
pub struct Adapter<T>(pub T);

impl<T> Adapter<T> {
    /// Creates a new adapter wrapping the given value.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Consumes the adapter and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Returns a reference to the inner value.
    pub fn as_inner(&self) -> &T {
        &self.0
    }
}

impl<T> Serialize for Adapter<T>
where
    T: Facet<'static>,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let peek = Peek::new(&self.0);
        PeekSerialize(peek).serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Adapter<T>
where
    T: Facet<'static>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let partial = Partial::alloc_owned::<T>().map_err(serde::de::Error::custom)?;
        let seed = PartialSeed { partial };
        let partial = serde::de::DeserializeSeed::deserialize(seed, deserializer)?;
        let heap_value = partial.build().map_err(serde::de::Error::custom)?;
        let value: T = heap_value
            .materialize::<T>()
            .map_err(serde::de::Error::custom)?;
        Ok(Adapter(value))
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Adapter<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Adapter<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Clone> Clone for Adapter<T> {
    fn clone(&self) -> Self {
        Adapter(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64;
    use facet::Facet;
    use std::collections::HashMap;

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

    fn roundtrip<T>(value: T)
    where
        T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
    {
        let adapter = Adapter::new(value.clone());
        let json = serde_json::to_string(&adapter).expect("serialize");
        let back: Adapter<T> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.into_inner(), value);
    }

    #[test]
    fn test_primitives() {
        roundtrip(42u32);
        roundtrip(true);
        roundtrip(f64::consts::PI);
        roundtrip("hello".to_string());
    }

    #[test]
    fn test_simple_struct() {
        roundtrip(Simple {
            name: "Alice".to_string(),
            age: 30,
            active: true,
        });
    }

    #[test]
    fn test_nested_struct() {
        roundtrip(Nested {
            label: "origin".to_string(),
            point: Point { x: 0.0, y: 0.0 },
        });
    }

    #[test]
    fn test_option_some() {
        roundtrip(WithOption {
            value: Some("hi".to_string()),
        });
    }

    #[test]
    fn test_option_none() {
        roundtrip(WithOption { value: None });
    }

    #[test]
    fn test_vec() {
        roundtrip(WithVec {
            items: vec![1, 2, 3],
        });
    }

    #[test]
    fn test_hashmap() {
        let mut data = HashMap::new();
        data.insert("a".to_string(), 1);
        data.insert("b".to_string(), 2);
        roundtrip(WithMap { data });
    }

    #[test]
    fn test_unit_enum() {
        roundtrip(Color::Red);
        roundtrip(Color::Green);
        roundtrip(Color::Blue);
    }

    #[test]
    fn test_complex_enum() {
        roundtrip(Shape::Circle(5.0));
        roundtrip(Shape::Rectangle {
            width: 3.0,
            height: 4.0,
        });
        roundtrip(Shape::Point);
    }

    #[test]
    fn test_serialize_output() {
        let s = Simple {
            name: "Bob".to_string(),
            age: 25,
            active: false,
        };
        let json = serde_json::to_string(&Adapter::new(s)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "Bob");
        assert_eq!(parsed["age"], 25);
        assert_eq!(parsed["active"], false);
    }
}
