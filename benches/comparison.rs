//! Serialization / deserialization benchmarks comparing three approaches on
//! the *same* Rust types:
//!
//! 1. **serde native** — types deriving `Serialize`/`Deserialize`,
//!    driven directly by `serde_json`.
//! 2. **facet native** — types deriving `Facet`, driven by `facet-json`
//!    (which walks the facet shape tree directly, no serde in the middle).
//! 3. **facet-serde** — types deriving `Facet`, wrapped in `Adapter<T>` and
//!    driven by `serde_json` through this crate's serde bridge.
//!
//! Run with:
//! ```text
//! cargo bench --bench comparison
//! ```
//!
//! Run a single group:
//! ```text
//! cargo bench --bench comparison -- serialize/point
//! ```

use std::collections::HashMap;
use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use facet::Facet;
use facet_serde::Adapter;
use serde::{Deserialize, Serialize};

// ── Test types ──────────────────────────────────────────────────────────
// Each type derives both `Facet` and `Serialize`/`Deserialize` so exactly
// the same in-memory layout is fed to all three benchmarked paths.

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Address {
    street: String,
    city: String,
    zip: String,
    country: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Person {
    name: String,
    age: u32,
    email: String,
    address: Address,
    tags: Vec<String>,
    scores: Vec<i32>,
    active: bool,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Company {
    name: String,
    founded: u32,
    employees: Vec<Person>,
    metadata: HashMap<String, String>,
}

// ── Sample data ─────────────────────────────────────────────────────────
//
// Sizes are chosen to expose realistic per-payload cost rather than
// microbenchmark noise:
//   * `point`   — single ~24 B struct, isolates dispatch overhead.
//   * `person`  — nested struct with small collections, ~50 fields total.
//   * `company` — 1 000 employees + 100 metadata entries → ~200 KB JSON.
//   * `points`  — 50 000-element `Vec<Point>` → ~1.2 MB JSON, exercises
//                 the list / scalar hot path.
//   * `matrix`  — 200 × 200 nested `Vec<Vec<f64>>` → ~800 KB JSON,
//                 exercises deeply nested list traversal.

const COMPANY_EMPLOYEES: usize = 1_000;
const COMPANY_METADATA_ENTRIES: usize = 100;
const BIG_POINTS: usize = 50_000;
const MATRIX_DIM: usize = 200;

fn sample_point() -> Point {
    Point {
        x: 1.5,
        y: -2.75,
        z: 42.125,
    }
}

fn sample_person() -> Person {
    Person {
        name: "Alice Johnson".to_string(),
        age: 34,
        email: "alice@example.com".to_string(),
        address: Address {
            street: "123 Main St".to_string(),
            city: "Springfield".to_string(),
            zip: "12345".to_string(),
            country: "USA".to_string(),
        },
        tags: vec![
            "admin".to_string(),
            "developer".to_string(),
            "reviewer".to_string(),
        ],
        scores: vec![95, 87, 92, 78, 100],
        active: true,
    }
}

fn sample_company() -> Company {
    let metadata = (0..COMPANY_METADATA_ENTRIES)
        .map(|i| (format!("key_{i:03}"), format!("value_{i:03}_lorem_ipsum")))
        .collect();

    Company {
        name: "Acme Corp".to_string(),
        founded: 1999,
        employees: (0..COMPANY_EMPLOYEES)
            .map(|i| {
                let mut p = sample_person();
                p.age = 20 + (i as u32 % 45);
                p.name = format!("Employee #{i:05}");
                p.email = format!("employee{i:05}@example.com");
                p
            })
            .collect(),
        metadata,
    }
}

fn sample_points() -> Vec<Point> {
    (0..BIG_POINTS)
        .map(|i| {
            let f = i as f64;
            Point {
                x: f * 0.5,
                y: f * -0.25 + 1.0,
                z: f.sin(),
            }
        })
        .collect()
}

fn sample_matrix() -> Vec<Vec<f64>> {
    (0..MATRIX_DIM)
        .map(|i| {
            (0..MATRIX_DIM)
                .map(|j| ((i * MATRIX_DIM + j) as f64).sqrt())
                .collect()
        })
        .collect()
}

// ── Benchmark harness ───────────────────────────────────────────────────

/// Run all three serialize variants on `value` under one benchmark group.
///
/// Each iteration receives a fresh clone of the input so that any move /
/// consumption cost (`Adapter::new` takes by value) is counted uniformly.
/// `PerIteration` batching keeps memory bounded for the multi-MB payloads
/// — `SmallInput`/`LargeInput` would preallocate a whole batch up front.
fn bench_serialize<T>(c: &mut Criterion, name: &str, value: T)
where
    T: Facet<'static> + Serialize + Clone,
{
    let mut group = c.benchmark_group(format!("serialize/{name}"));

    // Report throughput based on the encoded JSON size so results are
    // comparable across payload types.
    let baseline = serde_json::to_string(&value).expect("baseline size probe");
    group.throughput(criterion::Throughput::Bytes(baseline.len() as u64));

    group.bench_function("serde_native", |b| {
        b.iter_batched(
            || value.clone(),
            |v| black_box(serde_json::to_string(&v).unwrap()),
            BatchSize::PerIteration,
        );
    });

    group.bench_function("facet_native", |b| {
        b.iter_batched(
            || value.clone(),
            |v| black_box(facet_json::to_string(&v)),
            BatchSize::PerIteration,
        );
    });

    group.bench_function("facet_serde", |b| {
        b.iter_batched(
            || Adapter::new(value.clone()),
            |v| black_box(serde_json::to_string(&v).unwrap()),
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

/// Run all three deserialize variants against the same input string.
///
/// The input is generated once via `serde_json` so every path parses the
/// identical bytes. Throughput is reported in bytes-per-second of input
/// consumed, making cross-payload comparison meaningful.
fn bench_deserialize<T>(c: &mut Criterion, name: &str, value: T)
where
    T: Facet<'static> + Serialize + for<'de> Deserialize<'de>,
{
    let json = serde_json::to_string(&value).expect("baseline serialize");
    let mut group = c.benchmark_group(format!("deserialize/{name}"));
    group.throughput(criterion::Throughput::Bytes(json.len() as u64));

    group.bench_function("serde_native", |b| {
        b.iter(|| black_box(serde_json::from_str::<T>(&json).unwrap()));
    });

    group.bench_function("facet_native", |b| {
        b.iter(|| black_box(facet_json::from_str::<T>(&json).unwrap()));
    });

    group.bench_function("facet_serde", |b| {
        b.iter(|| black_box(serde_json::from_str::<Adapter<T>>(&json).unwrap()));
    });

    group.finish();
}

// ── Entry points ────────────────────────────────────────────────────────

fn serialize_benches(c: &mut Criterion) {
    bench_serialize(c, "point", sample_point());
    bench_serialize(c, "person", sample_person());
    bench_serialize(c, "company", sample_company());
    bench_serialize(c, "points_vec", sample_points());
    bench_serialize(c, "matrix", sample_matrix());
}

fn deserialize_benches(c: &mut Criterion) {
    bench_deserialize(c, "point", sample_point());
    bench_deserialize(c, "person", sample_person());
    bench_deserialize(c, "company", sample_company());
    bench_deserialize(c, "points_vec", sample_points());
    bench_deserialize(c, "matrix", sample_matrix());
}

/// Configure Criterion for the larger payloads: fewer samples so total
/// wall-time stays reasonable, but enough for statistical significance.
/// Individual measurements can be seconds long on the biggest cases.
fn config() -> Criterion {
    Criterion::default()
        .sample_size(30)
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(5))
}

criterion_group! {
    name = benches;
    config = config();
    targets = serialize_benches, deserialize_benches
}
criterion_main!(benches);
