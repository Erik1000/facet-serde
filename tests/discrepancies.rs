// Tests that verify potential ser/de discrepancies.
//
// Each test targets a specific mismatch between serialize and deserialize
// logic that could break roundtrip under certain conditions.

use facet::Facet;
use facet_serde::Adapter;
use std::collections::HashMap;

// ── Helper ──────────────────────────────────────────────────────────────

fn json_roundtrip<T>(value: T)
where
    T: Facet<'static> + std::fmt::Debug + PartialEq + Clone,
{
    let adapter = Adapter::new(value.clone());
    let json = serde_json::to_string(&adapter).expect("serialize");
    let back: Adapter<T> = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("deserialize failed: {e}\njson: {json}"));
    assert_eq!(back.into_inner(), value, "roundtrip failed for: {json}");
}

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

// =========================================================================
// Issue #1: serialize_struct announces field_count() (ALL fields) but only
//           emits fields_for_serialize() (which skips conditional fields).
//           If a field is skipped, the announced count exceeds actual output.
// =========================================================================

#[derive(Facet, Debug, PartialEq, Clone)]
struct SkipOptionNone {
    name: String,
    #[facet(skip_serializing_if = Option::is_none)]
    nickname: Option<String>,
    age: u32,
}

#[test]
fn json_struct_skip_serializing_if_none() {
    // When nickname is None, fields_for_serialize() skips it (2 fields),
    // but field_count() returns 3. The announced count may be wrong.
    json_roundtrip(SkipOptionNone {
        name: "Alice".into(),
        nickname: None,
        age: 30,
    });
}

#[test]
fn json_struct_skip_serializing_if_some() {
    // When nickname is Some, all 3 fields are emitted — no mismatch.
    json_roundtrip(SkipOptionNone {
        name: "Bob".into(),
        nickname: Some("Bobby".into()),
        age: 25,
    });
}

#[test]
fn ron_struct_skip_serializing_if_none() {
    // RON may enforce the announced field count more strictly.
    ron_roundtrip(SkipOptionNone {
        name: "Alice".into(),
        nickname: None,
        age: 30,
    });
}

#[test]
fn ron_struct_skip_serializing_if_some() {
    ron_roundtrip(SkipOptionNone {
        name: "Bob".into(),
        nickname: Some("Bobby".into()),
        age: 25,
    });
}

// Also test with a map value that may be empty
#[derive(Facet, Debug, PartialEq, Clone)]
struct SkipEmptyMap {
    label: String,
    #[facet(skip_serializing_if = HashMap::is_empty)]
    metadata: HashMap<String, String>,
}

#[test]
fn json_struct_skip_empty_map() {
    json_roundtrip(SkipEmptyMap {
        label: "test".into(),
        metadata: HashMap::new(),
    });
}

#[test]
fn json_struct_skip_nonempty_map() {
    let mut metadata = HashMap::new();
    metadata.insert("key".into(), "value".into());
    json_roundtrip(SkipEmptyMap {
        label: "test".into(),
        metadata,
    });
}

// =========================================================================
// Issue #2: StructKind::Tuple serializes with name "" but deserializes
//           expecting shape.effective_name() (e.g. "(i32, String)").
//           RON uses struct names and will fail on the mismatch.
// =========================================================================

#[test]
fn json_bare_tuple_roundtrip() {
    // JSON doesn't use struct names, so this should work even with the mismatch.
    json_roundtrip((42i32, "hello".to_string()));
}

#[test]
fn json_bare_tuple_nested_roundtrip() {
    json_roundtrip((1u8, 2u16, 3u32));
}

#[test]
fn ron_bare_tuple_roundtrip() {
    // RON uses struct names. ser emits "" but de expects effective_name().
    // This is the most likely test to fail due to the name mismatch.
    ron_roundtrip((42i32, "hello".to_string()));
}

#[test]
fn ron_bare_triple_roundtrip() {
    ron_roundtrip((1i32, 2i32, 3i32));
}

// Tuple inside a struct field — exercises Tuple serialization as a nested value.
#[derive(Facet, Debug, PartialEq, Clone)]
struct ContainsTuple {
    pair: (i32, String),
}

#[test]
fn json_tuple_in_struct_roundtrip() {
    json_roundtrip(ContainsTuple {
        pair: (10, "ten".into()),
    });
}

#[test]
fn ron_tuple_in_struct_roundtrip() {
    ron_roundtrip(ContainsTuple {
        pair: (10, "ten".into()),
    });
}

// =========================================================================
// Issue #3: Enum newtype variant detection uses fields_for_serialize().len()
//           on ser side but v.data.fields.len() on de side.
//           If a single-field tuple variant has its field skipped, ser sees
//           0 fields (→ serialize_tuple_variant) while de sees 1 field
//           (→ newtype_variant_seed).
// =========================================================================

// Note: skip_serializing_if on enum variant fields is unusual but technically
// possible. We approximate with a multi-field variant to verify the basic
// enum roundtrip is consistent.

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
enum MixedEnum {
    Unit,
    Newtype(String),
    Tuple(i32, i32),
    Struct { x: f64, y: f64 },
}

#[test]
fn json_enum_unit_roundtrip() {
    json_roundtrip(MixedEnum::Unit);
}

#[test]
fn json_enum_newtype_roundtrip() {
    json_roundtrip(MixedEnum::Newtype("hello".into()));
}

#[test]
fn json_enum_tuple_roundtrip() {
    json_roundtrip(MixedEnum::Tuple(1, 2));
}

#[test]
fn json_enum_struct_roundtrip() {
    json_roundtrip(MixedEnum::Struct { x: 1.0, y: 2.0 });
}

#[test]
fn ron_enum_unit_roundtrip() {
    ron_roundtrip(MixedEnum::Unit);
}

#[test]
fn ron_enum_newtype_roundtrip() {
    ron_roundtrip(MixedEnum::Newtype("hello".into()));
}

#[test]
fn ron_enum_tuple_roundtrip() {
    ron_roundtrip(MixedEnum::Tuple(1, 2));
}

#[test]
fn ron_enum_struct_roundtrip() {
    ron_roundtrip(MixedEnum::Struct { x: 1.0, y: 2.0 });
}

// =========================================================================
// Issue #4: TupleStruct field count — de uses st.fields.len() (all fields)
//           but ser uses fields_for_serialize().len(). If they differ, the
//           deserializer expects more positional elements than actually exist.
// =========================================================================

#[derive(Facet, Debug, PartialEq, Clone)]
struct NewtypeWrapper(String);

#[derive(Facet, Debug, PartialEq, Clone)]
struct TupleTwo(i32, String);

#[test]
fn json_newtype_wrapper_roundtrip() {
    json_roundtrip(NewtypeWrapper("wrapped".into()));
}

#[test]
fn json_tuple_struct_roundtrip() {
    json_roundtrip(TupleTwo(42, "hello".into()));
}

#[test]
fn ron_newtype_wrapper_roundtrip() {
    ron_roundtrip(NewtypeWrapper("wrapped".into()));
}

#[test]
fn ron_tuple_struct_roundtrip() {
    ron_roundtrip(TupleTwo(42, "hello".into()));
}

// =========================================================================
// Issue #6: Stale comment — serialize_struct says "Use SerializeMap" but
//           actually uses SerializeStruct. This is cosmetic and untestable,
//           but we verify the actual output format is struct-like (not map).
// =========================================================================

#[test]
fn json_struct_output_format() {
    // Verify that named structs produce JSON objects with proper field names,
    // confirming SerializeStruct (not SerializeMap) behavior.
    #[derive(Facet, Debug, PartialEq, Clone)]
    struct TwoFields {
        alpha: i32,
        beta: String,
    }

    let adapter = Adapter::new(TwoFields {
        alpha: 1,
        beta: "two".into(),
    });
    let json = serde_json::to_string(&adapter).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Should be an object with named fields, not an array
    assert!(parsed.is_object());
    assert_eq!(parsed["alpha"], 1);
    assert_eq!(parsed["beta"], "two");
}

// =========================================================================
// Combined stress: skip + nested containers + enums
// =========================================================================

#[derive(Facet, Debug, PartialEq, Clone)]
struct Complex {
    name: String,
    #[facet(skip_serializing_if = Option::is_none)]
    tag: Option<String>,
    #[facet(skip_serializing_if = Vec::is_empty)]
    items: Vec<i32>,
    #[facet(skip_serializing_if = HashMap::is_empty)]
    meta: HashMap<String, String>,
}

#[test]
fn json_complex_all_empty() {
    // All skippable fields are empty/None — maximum field count divergence.
    json_roundtrip(Complex {
        name: "bare".into(),
        tag: None,
        items: vec![],
        meta: HashMap::new(),
    });
}

#[test]
fn json_complex_all_present() {
    let mut meta = HashMap::new();
    meta.insert("k".into(), "v".into());
    json_roundtrip(Complex {
        name: "full".into(),
        tag: Some("tagged".into()),
        items: vec![1, 2, 3],
        meta,
    });
}

#[test]
fn json_complex_partial() {
    json_roundtrip(Complex {
        name: "partial".into(),
        tag: Some("yes".into()),
        items: vec![],
        meta: HashMap::new(),
    });
}

#[test]
fn ron_complex_all_empty() {
    ron_roundtrip(Complex {
        name: "bare".into(),
        tag: None,
        items: vec![],
        meta: HashMap::new(),
    });
}

#[test]
fn ron_complex_all_present() {
    let mut meta = HashMap::new();
    meta.insert("k".into(), "v".into());
    ron_roundtrip(Complex {
        name: "full".into(),
        tag: Some("tagged".into()),
        items: vec![1, 2, 3],
        meta,
    });
}
