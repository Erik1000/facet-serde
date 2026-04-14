# facet-serde

This crate was mainly created by Claude Opus.

A bridge between [facet](https://crates.io/crates/facet) and [serde](https://crates.io/crates/serde).

`facet-serde` provides `Adapter<T>`, a newtype wrapper that implements
`serde::Serialize` and `serde::Deserialize` for any `T: Facet<'static>`.
Serialization is driven by `facet_reflect::Peek`; deserialization is driven
by `facet_reflect::Partial`.

This allows facet-only types to be used with the entire serde ecosystem
(serde_json, serde_yaml, bincode, postcard, …) without deriving
`serde::Serialize` / `serde::Deserialize` on those types.

## Usage

```toml
[dependencies]
facet-serde = "0.1"
facet = "0.44"
serde_json = "1" # or any other serde format
```

```rust
use facet::Facet;
use facet_serde::Adapter;

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

let point = Point { x: 1.0, y: 2.0 };

// Serialize
let json = serde_json::to_string(&Adapter::new(point)).unwrap();
assert_eq!(json, r#"{"x":1.0,"y":2.0}"#);

// Deserialize
let adapter: Adapter<Point> = serde_json::from_str(&json).unwrap();
assert_eq!(adapter.into_inner(), Point { x: 1.0, y: 2.0 });
```

## Supported types

Anything with a facet `Shape` works: scalars, structs (named, tuple, unit),
enums (unit, newtype, tuple, struct variants), `Option`, `Vec`, `HashMap`,
`Box`, and nested combinations of the above.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0)>
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.