use facet::{Def, Facet, FieldFlags, Shape, StructKind, Type, UserType};
use facet_reflect::{Partial, ScalarType};
use serde::Deserializer;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use std::collections::HashMap;
use std::fmt;
use std::sync::{OnceLock, RwLock};

/// Cache mapping the address of a static `StructType` / `Variant` to a
/// leaked `&'static [&'static str]` of its field or variant names.
///
/// Serde's `deserialize_struct` / `deserialize_enum` require `&'static`
/// slices, but facet only exposes `&'static [Field]` / `&'static [Variant]`
/// — the individual names are already `'static`, only the slice layout has
/// to be materialized once and leaked.
///
/// Access is overwhelmingly read-heavy: each unique shape triggers one
/// write, all subsequent deserializations of the same shape are pure reads.
/// A `RwLock` lets those reads run concurrently across threads (a plain
/// `Mutex` would needlessly serialize them). A thread-local would avoid the
/// lock entirely but at the cost of leaking a fresh `Vec` per (thread, shape)
/// pair, which is worse for long-lived multi-threaded services.
fn cached_static_str_slice(
    key: usize,
    compute: impl FnOnce() -> Vec<&'static str>,
) -> &'static [&'static str] {
    static CACHE: OnceLock<RwLock<HashMap<usize, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    // Fast path: shared read lock. After warmup, this is the only path taken.
    if let Some(&slice) = cache.read().unwrap().get(&key) {
        return slice;
    }

    // Slow path: upgrade to a write lock. Re-check in case another thread
    // populated the entry between our read and write acquisitions.
    let mut map = cache.write().unwrap();
    map.entry(key).or_insert_with(|| Vec::leak(compute()))
}

/// A [`DeserializeSeed`] implementation that drives a [`Partial`] builder.
///
/// Uses `Partial<'static, false>` (owned allocation) so that scalar `set`
/// calls work without lifetime issues.
pub(crate) struct PartialSeed {
    pub partial: Partial<'static, false>,
}

impl<'de> DeserializeSeed<'de> for PartialSeed {
    type Value = Partial<'static, false>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let shape = self.partial.shape();

        // Check definition-based types first (Option, Result, List, etc.)
        // These take priority because e.g. Option<T> has both Def::Option and
        // Type::User(UserType::Enum), and must use begin_some/visit_none rather
        // than the generic enum visitor.
        match &shape.def {
            Def::Scalar => {
                let visitor = ScalarVisitor {
                    partial: self.partial,
                };
                return deserialize_scalar(visitor, shape, deserializer);
            }
            Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                let visitor = SeqVisitor {
                    partial: self.partial,
                };
                return deserializer.deserialize_seq(visitor);
            }
            Def::Set(_) => {
                let visitor = SetVisitor {
                    partial: self.partial,
                };
                return deserializer.deserialize_seq(visitor);
            }
            Def::Map(_) => {
                let visitor = MapDeVisitor {
                    partial: self.partial,
                };
                return deserializer.deserialize_map(visitor);
            }
            Def::Option(_) => {
                let visitor = OptionVisitor {
                    partial: self.partial,
                };
                return deserializer.deserialize_option(visitor);
            }
            Def::Result(_) => {
                let visitor = EnumVisitor {
                    partial: self.partial,
                };
                return deserializer.deserialize_enum("Result", &["Ok", "Err"], visitor);
            }
            Def::Pointer(_) => {
                let partial = self.partial.begin_smart_ptr().map_err(de::Error::custom)?;
                let partial = PartialSeed { partial }.deserialize(deserializer)?;
                let partial = partial.end().map_err(de::Error::custom)?;
                return Ok(partial);
            }
            _ => {}
        }

        // Then check user types (struct/enum)
        match &shape.ty {
            Type::User(UserType::Struct(st)) => match st.kind {
                StructKind::Struct => {
                    let field_names =
                        cached_static_str_slice(std::ptr::from_ref(st) as usize, || {
                            st.fields
                                .iter()
                                .map(|f: &facet::Field| f.effective_name())
                                .collect()
                        });
                    let visitor = StructVisitor {
                        partial: self.partial,
                    };
                    deserializer.deserialize_struct(shape.effective_name(), field_names, visitor)
                }
                StructKind::TupleStruct => {
                    let len = st.fields.len();
                    let visitor = TupleStructVisitor {
                        partial: self.partial,
                        field_count: len,
                    };
                    deserializer.deserialize_tuple_struct(shape.effective_name(), len, visitor)
                }
                StructKind::Tuple => {
                    let len = st.fields.len();
                    let visitor = TupleStructVisitor {
                        partial: self.partial,
                        field_count: len,
                    };
                    deserializer.deserialize_tuple(len, visitor)
                }
                StructKind::Unit => {
                    let visitor = UnitStructVisitor {
                        partial: self.partial,
                    };
                    deserializer.deserialize_unit_struct(shape.effective_name(), visitor)
                }
            },
            Type::User(UserType::Enum(et)) => {
                let variant_names =
                    cached_static_str_slice(std::ptr::from_ref(et) as usize, || {
                        et.variants.iter().map(|v| v.effective_name()).collect()
                    });
                let visitor = EnumVisitor {
                    partial: self.partial,
                };
                deserializer.deserialize_enum(shape.effective_name(), variant_names, visitor)
            }
            _ => Err(de::Error::custom(format!(
                "unsupported facet shape for deserialization: {:?}",
                shape.def
            ))),
        }
    }
}

fn deserialize_scalar<'de, D: Deserializer<'de>>(
    visitor: ScalarVisitor,
    shape: &'static Shape,
    deserializer: D,
) -> Result<Partial<'static, false>, D::Error> {
    match shape.scalar_type() {
        Some(ScalarType::Bool) => deserializer.deserialize_bool(visitor),
        Some(ScalarType::U8) => deserializer.deserialize_u8(visitor),
        Some(ScalarType::U16) => deserializer.deserialize_u16(visitor),
        Some(ScalarType::U32) => deserializer.deserialize_u32(visitor),
        Some(ScalarType::U64) => deserializer.deserialize_u64(visitor),
        Some(ScalarType::U128) => deserializer.deserialize_u128(visitor),
        Some(ScalarType::USize) => deserializer.deserialize_u64(visitor),
        Some(ScalarType::I8) => deserializer.deserialize_i8(visitor),
        Some(ScalarType::I16) => deserializer.deserialize_i16(visitor),
        Some(ScalarType::I32) => deserializer.deserialize_i32(visitor),
        Some(ScalarType::I64) => deserializer.deserialize_i64(visitor),
        Some(ScalarType::I128) => deserializer.deserialize_i128(visitor),
        Some(ScalarType::ISize) => deserializer.deserialize_i64(visitor),
        Some(ScalarType::F32) => deserializer.deserialize_f32(visitor),
        Some(ScalarType::F64) => deserializer.deserialize_f64(visitor),
        Some(ScalarType::Char) => deserializer.deserialize_char(visitor),
        Some(ScalarType::Str | ScalarType::String | ScalarType::CowStr) => {
            deserializer.deserialize_str(visitor)
        }
        Some(ScalarType::Unit) => deserializer.deserialize_unit(visitor),
        _ => deserializer.deserialize_str(visitor),
    }
}

// ── Scalar Visitor ──────────────────────────────────────────────────────

struct ScalarVisitor {
    partial: Partial<'static, false>,
}

impl ScalarVisitor {
    fn set_value<T: Facet<'static>>(
        self,
        v: T,
    ) -> Result<Partial<'static, false>, facet_reflect::ReflectError> {
        self.partial.set(v)
    }
}

impl<'de> Visitor<'de> for ScalarVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a scalar value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_i8<E: de::Error>(self, v: i8) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_i16<E: de::Error>(self, v: i16) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_i32<E: de::Error>(self, v: i32) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        match self.partial.shape().scalar_type() {
            Some(ScalarType::I8) => self.set_value(v as i8).map_err(de::Error::custom),
            Some(ScalarType::I16) => self.set_value(v as i16).map_err(de::Error::custom),
            Some(ScalarType::I32) => self.set_value(v as i32).map_err(de::Error::custom),
            Some(ScalarType::ISize) => self.set_value(v as isize).map_err(de::Error::custom),
            Some(ScalarType::F32) => self.set_value(v as f32).map_err(de::Error::custom),
            Some(ScalarType::F64) => self.set_value(v as f64).map_err(de::Error::custom),
            _ => self.set_value(v).map_err(de::Error::custom),
        }
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_u8<E: de::Error>(self, v: u8) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_u16<E: de::Error>(self, v: u16) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_u32<E: de::Error>(self, v: u32) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        match self.partial.shape().scalar_type() {
            Some(ScalarType::U8) => self.set_value(v as u8).map_err(de::Error::custom),
            Some(ScalarType::U16) => self.set_value(v as u16).map_err(de::Error::custom),
            Some(ScalarType::U32) => self.set_value(v as u32).map_err(de::Error::custom),
            Some(ScalarType::USize) => self.set_value(v as usize).map_err(de::Error::custom),
            Some(ScalarType::I8) => self.set_value(v as i8).map_err(de::Error::custom),
            Some(ScalarType::I16) => self.set_value(v as i16).map_err(de::Error::custom),
            Some(ScalarType::I32) => self.set_value(v as i32).map_err(de::Error::custom),
            Some(ScalarType::I64) => self.set_value(v as i64).map_err(de::Error::custom),
            Some(ScalarType::ISize) => self.set_value(v as isize).map_err(de::Error::custom),
            Some(ScalarType::F32) => self.set_value(v as f32).map_err(de::Error::custom),
            Some(ScalarType::F64) => self.set_value(v as f64).map_err(de::Error::custom),
            _ => self.set_value(v).map_err(de::Error::custom),
        }
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        if self.partial.shape().scalar_type() == Some(ScalarType::F32) {
            self.set_value(v as f32).map_err(de::Error::custom)
        } else {
            self.set_value(v).map_err(de::Error::custom)
        }
    }

    fn visit_char<E: de::Error>(self, v: char) -> Result<Self::Value, E> {
        self.set_value(v).map_err(de::Error::custom)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        match self.partial.shape().scalar_type() {
            Some(ScalarType::String | ScalarType::Str | ScalarType::CowStr) => {
                self.set_value(v.to_owned()).map_err(de::Error::custom)
            }
            _ => self.partial.parse_from_str(v).map_err(de::Error::custom),
        }
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        match self.partial.shape().scalar_type() {
            Some(ScalarType::String | ScalarType::Str | ScalarType::CowStr) => {
                self.set_value(v).map_err(de::Error::custom)
            }
            _ => self.partial.parse_from_str(&v).map_err(de::Error::custom),
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.set_value(()).map_err(de::Error::custom)
    }
}

// ── Unit Struct Visitor ─────────────────────────────────────────────────

struct UnitStructVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for UnitStructVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a unit struct")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(self.partial)
    }
}

// ── Struct Visitor (named fields, via map access) ───────────────────────

struct StructVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for StructVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a struct")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut partial = self.partial;
        let mut visited = Vec::new();

        // Check for flattened fields upfront. If any exist, enable deferred mode so
        // that re-entering a partially-built flatten frame restores its state rather
        // than starting fresh.
        let has_flatten = {
            let shape = partial.shape();
            if let Type::User(UserType::Struct(st)) = &shape.ty {
                st.fields
                    .iter()
                    .any(|f| f.flags.contains(FieldFlags::FLATTEN))
            } else {
                false
            }
        };
        if has_flatten {
            partial = partial.begin_deferred().map_err(de::Error::custom)?;
        }

        while let Some(key) = map.next_key::<String>()? {
            visited.push(key.clone());
            // Check if this key is a direct field of the current struct.
            if partial.field_index(&key).is_some() {
                partial = partial.begin_field(&key).map_err(de::Error::custom)?;
                partial = map.next_value_seed(PartialSeed { partial })?;
                partial = partial.end().map_err(de::Error::custom)?;
            } else {
                // Look for the key inside a flattened field.
                let shape = partial.shape();
                let flatten_field_name = if let Type::User(UserType::Struct(st)) = &shape.ty {
                    st.fields
                        .iter()
                        .filter(|f| f.flags.contains(FieldFlags::FLATTEN))
                        .find(|f| {
                            if let Type::User(UserType::Struct(inner_st)) = &f.shape().ty {
                                inner_st.fields.iter().any(|sf| sf.effective_name() == key)
                            } else {
                                false
                            }
                        })
                        .map(|f| f.effective_name())
                } else {
                    None
                };
                if let Some(flatten_name) = flatten_field_name {
                    // Also mark the flatten field itself as "visited" for defaults.
                    if !visited.iter().any(|v: &String| v == flatten_name) {
                        visited.push(flatten_name.to_owned());
                    }
                    // In deferred mode, begin_field on an already-stored flatten frame
                    // restores it so we can add the next sub-field.
                    partial = partial
                        .begin_field(flatten_name)
                        .map_err(de::Error::custom)?;
                    partial = partial.begin_field(&key).map_err(de::Error::custom)?;
                    partial = map.next_value_seed(PartialSeed { partial })?;
                    partial = partial.end().map_err(de::Error::custom)?;
                    // end() in deferred mode stores the flatten frame for re-entry.
                    partial = partial.end().map_err(de::Error::custom)?;
                } else {
                    // Unknown key — consume and discard the value.
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        if has_flatten {
            partial = partial.finish_deferred().map_err(de::Error::custom)?;
        }
        // Set defaults for fields that were not present in the data.
        // This handles fields skipped during serialization (e.g. skip_serializing_if).
        let shape = partial.shape();
        if let Type::User(UserType::Struct(st)) = &shape.ty {
            for (idx, field) in st.fields.iter().enumerate() {
                if !visited.iter().any(|v| v == field.effective_name()) {
                    // Only attempt if the field's type supports Default.
                    let can_default = field
                        .shape()
                        .type_ops
                        .is_some_and(|ops| ops.has_default_in_place());
                    if can_default {
                        partial = partial
                            .set_nth_field_to_default(idx)
                            .map_err(de::Error::custom)?;
                    }
                }
            }
        }
        Ok(partial)
    }
}

// ── Tuple Struct Visitor (positional fields, via seq access) ────────────

struct TupleStructVisitor {
    partial: Partial<'static, false>,
    field_count: usize,
}

impl<'de> Visitor<'de> for TupleStructVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a tuple struct")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let field_count = self.field_count;
        let mut partial = self.partial;
        for idx in 0..field_count {
            partial = partial.begin_nth_field(idx).map_err(de::Error::custom)?;
            let seed = PartialSeed { partial };
            match seq.next_element_seed(seed)? {
                Some(p) => {
                    partial = p;
                    partial = partial.end().map_err(de::Error::custom)?;
                }
                None => {
                    return Err(de::Error::custom(format!(
                        "expected element at index {idx}, but sequence ended"
                    )));
                }
            }
        }
        Ok(partial)
    }
}

// ── Specialized Seeds ───────────────────────────────────────────────────
// These seeds wrap a Partial and call begin_* inside their deserialize
// method, so the partial is only consumed when there's actual data.

/// A shared cell for passing `Partial` through serde seeds that may or
/// may not be consumed (when `next_element_seed` returns `None`, the seed
/// is dropped without calling `deserialize`, so we need to recover the
/// `Partial` from the drop).
struct SharedPartial {
    inner: std::rc::Rc<std::cell::RefCell<Option<Partial<'static, false>>>>,
}

impl SharedPartial {
    fn new(partial: Partial<'static, false>) -> Self {
        Self {
            inner: std::rc::Rc::new(std::cell::RefCell::new(Some(partial))),
        }
    }

    fn take(&self) -> Partial<'static, false> {
        self.inner
            .borrow_mut()
            .take()
            .expect("partial already taken")
    }

    fn put(&self, partial: Partial<'static, false>) {
        *self.inner.borrow_mut() = Some(partial);
    }

    fn clone_handle(&self) -> Self {
        Self {
            inner: std::rc::Rc::clone(&self.inner),
        }
    }

    fn into_inner(self) -> Partial<'static, false> {
        std::rc::Rc::try_unwrap(self.inner)
            .ok()
            .expect("shared partial still has references")
            .into_inner()
            .expect("partial was not returned")
    }
}

struct ListItemSeed {
    shared: SharedPartial,
}

impl<'de> DeserializeSeed<'de> for ListItemSeed {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let partial = self.shared.take();
        let partial = partial.begin_list_item().map_err(de::Error::custom)?;
        let partial = PartialSeed { partial }.deserialize(deserializer)?;
        let partial = partial.end().map_err(de::Error::custom)?;
        self.shared.put(partial);
        Ok(())
    }
}

struct SetItemSeed {
    shared: SharedPartial,
}

impl<'de> DeserializeSeed<'de> for SetItemSeed {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let partial = self.shared.take();
        let partial = partial.begin_set_item().map_err(de::Error::custom)?;
        let partial = PartialSeed { partial }.deserialize(deserializer)?;
        let partial = partial.end().map_err(de::Error::custom)?;
        self.shared.put(partial);
        Ok(())
    }
}

struct MapKeySeed {
    shared: SharedPartial,
}

impl<'de> DeserializeSeed<'de> for MapKeySeed {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let partial = self.shared.take();
        let partial = partial.begin_key().map_err(de::Error::custom)?;
        let partial = PartialSeed { partial }.deserialize(deserializer)?;
        let partial = partial.end().map_err(de::Error::custom)?;
        self.shared.put(partial);
        Ok(())
    }
}

struct MapValueSeed {
    shared: SharedPartial,
}

impl<'de> DeserializeSeed<'de> for MapValueSeed {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let partial = self.shared.take();
        let partial = partial.begin_value().map_err(de::Error::custom)?;
        let partial = PartialSeed { partial }.deserialize(deserializer)?;
        let partial = partial.end().map_err(de::Error::custom)?;
        self.shared.put(partial);
        Ok(())
    }
}

// ── Sequence Visitor (List/Array) ───────────────────────────────────────

struct SeqVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for SeqVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let is_array = matches!(self.partial.shape().def, Def::Array(_));
        let partial = if is_array {
            self.partial.init_array().map_err(de::Error::custom)?
        } else {
            self.partial.init_list().map_err(de::Error::custom)?
        };

        let shared = SharedPartial::new(partial);
        while seq
            .next_element_seed(ListItemSeed {
                shared: shared.clone_handle(),
            })?
            .is_some()
        {}

        Ok(shared.into_inner())
    }
}

// ── Set Visitor ─────────────────────────────────────────────────────────

struct SetVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for SetVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a set")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let partial = self.partial.init_set().map_err(de::Error::custom)?;

        let shared = SharedPartial::new(partial);
        while seq
            .next_element_seed(SetItemSeed {
                shared: shared.clone_handle(),
            })?
            .is_some()
        {}

        Ok(shared.into_inner())
    }
}

// ── Map Visitor ─────────────────────────────────────────────────────────

struct MapDeVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for MapDeVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a map")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let partial = self.partial.init_map().map_err(de::Error::custom)?;

        let shared = SharedPartial::new(partial);
        while map
            .next_key_seed(MapKeySeed {
                shared: shared.clone_handle(),
            })?
            .is_some()
        {
            map.next_value_seed(MapValueSeed {
                shared: shared.clone_handle(),
            })?;
        }

        Ok(shared.into_inner())
    }
}

// ── Option Visitor ──────────────────────────────────────────────────────

struct OptionVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for OptionVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "an option")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(self.partial)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let partial = self.partial.begin_some().map_err(de::Error::custom)?;
        let partial = PartialSeed { partial }.deserialize(deserializer)?;
        let partial = partial.end().map_err(de::Error::custom)?;
        Ok(partial)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.visit_none()
    }
}

// ── Enum Visitor ────────────────────────────────────────────────────────

struct VariantNameSeed;

impl<'de> DeserializeSeed<'de> for VariantNameSeed {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        deserializer.deserialize_identifier(VariantNameVisitor)
    }
}

struct VariantNameVisitor;

impl<'de> Visitor<'de> for VariantNameVisitor {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a variant name")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.to_owned())
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
        Ok(v.to_string())
    }
}

struct EnumVisitor {
    partial: Partial<'static, false>,
}

impl<'de> Visitor<'de> for EnumVisitor {
    type Value = Partial<'static, false>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "an enum")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error> {
        let (variant_name, variant_access) = data.variant_seed(VariantNameSeed)?;

        let mut partial = self
            .partial
            .select_variant_named(&variant_name)
            .map_err(de::Error::custom)?;

        // Determine the variant kind from the shape
        let (variant_kind, field_count) = {
            let (_, v) = partial
                .find_variant(&variant_name)
                .ok_or_else(|| de::Error::custom(format!("unknown variant: {}", variant_name)))?;
            (v.data.kind, v.data.fields.len())
        };

        match variant_kind {
            StructKind::Unit => {
                variant_access.unit_variant()?;
            }
            StructKind::TupleStruct if field_count == 1 => {
                // Newtype variant
                partial = partial.begin_nth_field(0).map_err(de::Error::custom)?;
                partial = variant_access.newtype_variant_seed(PartialSeed { partial })?;
                partial = partial.end().map_err(de::Error::custom)?;
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                let visitor = TupleStructVisitor {
                    partial,
                    field_count,
                };
                partial = variant_access.tuple_variant(field_count, visitor)?;
            }
            StructKind::Struct => {
                let field_names = {
                    let (_, v) = partial.find_variant(&variant_name).unwrap();
                    cached_static_str_slice(std::ptr::from_ref(v) as usize, || {
                        v.data
                            .fields
                            .iter()
                            .map(|f: &facet::Field| f.effective_name())
                            .collect()
                    })
                };
                let visitor = StructVisitor { partial };
                partial = variant_access.struct_variant(field_names, visitor)?;
            }
        }

        Ok(partial)
    }
}
