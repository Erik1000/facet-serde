use std::sync::{Arc, Mutex, RwLock};

use facet::Facet;

// ── Test types ──────────────────────────────────────────────────────────

#[derive(Facet, Debug)]
struct WithRwLock {
    value: RwLock<String>,
}

#[derive(Facet, Debug)]
struct WithMutex {
    value: Mutex<u32>,
}

#[derive(Facet, Debug)]
struct WithArcRwLock {
    value: Arc<RwLock<String>>,
}

#[derive(Facet, Debug)]
struct WithArcMutex {
    value: Arc<Mutex<i64>>,
}

#[derive(Facet, Debug)]
struct NestedLocks {
    inner: RwLock<WithMutex>,
}

#[derive(Facet, Debug)]
struct DeeplyNested {
    data: Arc<RwLock<Arc<Mutex<String>>>>,
}

#[derive(Facet, Debug)]
struct MultiField {
    name: RwLock<String>,
    count: Mutex<u32>,
    items: RwLock<Vec<i32>>,
}

#[derive(Facet, Debug)]
struct WithOptionLock {
    value: Option<RwLock<String>>,
}

#[derive(Facet, Debug)]
struct WithVecOfLocks {
    items: Vec<RwLock<u32>>,
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn serialize_json<T: Facet<'static>>(value: &T) -> String {
    let mut buf = Vec::new();
    let ser = &mut serde_json::Serializer::new(&mut buf);
    facet_serde::serialize(value, ser).unwrap();
    String::from_utf8(buf).unwrap()
}

fn assert_json_eq(actual: &str, expected: &serde_json::Value) {
    let actual_val: serde_json::Value = serde_json::from_str(actual).unwrap();
    assert_eq!(&actual_val, expected, "json mismatch:\n  got: {actual}");
}

// ── RwLock tests ────────────────────────────────────────────────────────

#[test]
fn serialize_rwlock_field() {
    let v = WithRwLock {
        value: RwLock::new("hello".to_string()),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": "hello"}));
}

#[test]
fn serialize_rwlock_via_adapter() {
    let v = RwLock::new(42u32);
    let json = serialize_json(&v);
    assert_eq!(json, "42");
}

// ── Mutex tests ─────────────────────────────────────────────────────────

#[test]
fn serialize_mutex_field() {
    let v = WithMutex {
        value: Mutex::new(99),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": 99}));
}

#[test]
fn serialize_mutex_via_adapter() {
    let v = Mutex::new("locked".to_string());
    let json = serialize_json(&v);
    assert_eq!(json, "\"locked\"");
}

// ── Arc + lock tests ───────────────────────────────────────────────────

#[test]
fn serialize_arc_rwlock() {
    let v = WithArcRwLock {
        value: Arc::new(RwLock::new("arc-rw".to_string())),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": "arc-rw"}));
}

#[test]
fn serialize_arc_mutex() {
    let v = WithArcMutex {
        value: Arc::new(Mutex::new(777)),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": 777}));
}

// ── Nested lock tests ──────────────────────────────────────────────────

#[test]
fn serialize_nested_rwlock_mutex() {
    let v = NestedLocks {
        inner: RwLock::new(WithMutex {
            value: Mutex::new(42),
        }),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"inner": {"value": 42}}));
}

#[test]
fn serialize_deeply_nested_arc_rwlock_arc_mutex() {
    let v = DeeplyNested {
        data: Arc::new(RwLock::new(Arc::new(Mutex::new("deep".to_string())))),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"data": "deep"}));
}

// ── Multiple lock fields ───────────────────────────────────────────────

#[test]
fn serialize_multi_field_locks() {
    let v = MultiField {
        name: RwLock::new("Alice".to_string()),
        count: Mutex::new(10),
        items: RwLock::new(vec![1, 2, 3]),
    };
    let json = serialize_json(&v);
    assert_json_eq(
        &json,
        &serde_json::json!({"name": "Alice", "count": 10, "items": [1, 2, 3]}),
    );
}

// ── Locks inside Option/Vec ────────────────────────────────────────────

#[test]
fn serialize_option_rwlock_some() {
    let v = WithOptionLock {
        value: Some(RwLock::new("present".to_string())),
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": "present"}));
}

#[test]
fn serialize_option_rwlock_none() {
    let v = WithOptionLock { value: None };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"value": null}));
}

#[test]
fn serialize_vec_of_rwlocks() {
    let v = WithVecOfLocks {
        items: vec![RwLock::new(10), RwLock::new(20), RwLock::new(30)],
    };
    let json = serialize_json(&v);
    assert_json_eq(&json, &serde_json::json!({"items": [10, 20, 30]}));
}

// ── Bare lock (not in a struct) ────────────────────────────────────────

#[test]
fn serialize_bare_arc_rwlock() {
    let v = Arc::new(RwLock::new(vec![1u32, 2, 3]));
    let json = serialize_json(&v);
    assert_eq!(json, "[1,2,3]");
}

#[test]
fn serialize_bare_arc_mutex_string() {
    let v = Arc::new(Mutex::new("mutex-val".to_string()));
    let json = serialize_json(&v);
    assert_eq!(json, "\"mutex-val\"");
}

// ── Shared Arc: same data, serialize twice ─────────────────────────────

#[test]
fn serialize_shared_arc_rwlock_twice() {
    let shared = Arc::new(RwLock::new(100u32));
    let json1 = serialize_json(&shared);
    let json2 = serialize_json(&shared);
    assert_eq!(json1, "100");
    assert_eq!(json2, "100");
}

// ── Module-level serialize function ────────────────────────────────────

#[test]
fn serialize_rwlock_via_module_fn() {
    let v = RwLock::new("module-fn".to_string());
    let mut buf = Vec::new();
    let ser = &mut serde_json::Serializer::new(&mut buf);
    facet_serde::serialize(&v, ser).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), "\"module-fn\"");
}
