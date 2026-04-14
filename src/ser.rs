use facet_core::{Def, Shape, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, PeekEnum, ScalarType};
use serde::ser::{SerializeMap, SerializeSeq, SerializeTupleStruct, SerializeTupleVariant};
use serde::{Serialize, Serializer};

/// A wrapper around [`Peek`] that implements [`serde::Serialize`].
///
/// This is the core serialization driver: it walks the facet shape tree
/// for an arbitrary value and translates it into serde serializer calls.
pub(crate) struct PeekSerialize<'mem, 'facet>(pub Peek<'mem, 'facet>);

impl<'mem, 'facet> Serialize for PeekSerialize<'mem, 'facet> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let peek = self.0.innermost_peek();
        serialize_peek(peek, serializer)
    }
}

fn serialize_peek<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let shape = peek.shape();

    // Check definition-based types first (Option, Result, List, etc.)
    // These take priority because e.g. Option<T> has both Def::Option and
    // Type::User(UserType::Enum), and must use serialize_option rather than
    // the generic enum serializer.
    match &shape.def {
        Def::Scalar => return serialize_scalar(peek, serializer),
        Def::List(_) | Def::Array(_) | Def::Slice(_) => return serialize_seq(peek, serializer),
        Def::Set(_) => return serialize_set(peek, serializer),
        Def::Map(_) => return serialize_map(peek, serializer),
        Def::Option(_) => return serialize_option(peek, serializer),
        Def::Result(_) => return serialize_result(peek, serializer),
        Def::Pointer(_) => return serialize_pointer(peek, serializer),
        _ => {}
    }

    // Then check user types (struct/enum)
    match &shape.ty {
        Type::User(UserType::Struct(st)) => serialize_struct(peek, st.kind, shape, serializer),
        Type::User(UserType::Enum(_)) => {
            let pe = peek.into_enum().map_err(serde::ser::Error::custom)?;
            serialize_enum(pe, shape, serializer)
        }
        _ => Err(serde::ser::Error::custom(format!(
            "unsupported facet shape: {:?}",
            shape.def
        ))),
    }
}

fn serialize_scalar<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => serializer.serialize_unit(),
        Some(ScalarType::Bool) => {
            let v = peek.get::<bool>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_bool(*v)
        }
        Some(ScalarType::Char) => {
            let v = peek.get::<char>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_char(*v)
        }
        Some(ScalarType::U8) => {
            let v = peek.get::<u8>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u8(*v)
        }
        Some(ScalarType::U16) => {
            let v = peek.get::<u16>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u16(*v)
        }
        Some(ScalarType::U32) => {
            let v = peek.get::<u32>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u32(*v)
        }
        Some(ScalarType::U64) => {
            let v = peek.get::<u64>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u64(*v)
        }
        Some(ScalarType::U128) => {
            let v = peek.get::<u128>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u128(*v)
        }
        Some(ScalarType::USize) => {
            let v = peek.get::<usize>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_u64(*v as u64)
        }
        Some(ScalarType::I8) => {
            let v = peek.get::<i8>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i8(*v)
        }
        Some(ScalarType::I16) => {
            let v = peek.get::<i16>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i16(*v)
        }
        Some(ScalarType::I32) => {
            let v = peek.get::<i32>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i32(*v)
        }
        Some(ScalarType::I64) => {
            let v = peek.get::<i64>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i64(*v)
        }
        Some(ScalarType::I128) => {
            let v = peek.get::<i128>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i128(*v)
        }
        Some(ScalarType::ISize) => {
            let v = peek.get::<isize>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_i64(*v as i64)
        }
        Some(ScalarType::F32) => {
            let v = peek.get::<f32>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_f32(*v)
        }
        Some(ScalarType::F64) => {
            let v = peek.get::<f64>().map_err(serde::ser::Error::custom)?;
            serializer.serialize_f64(*v)
        }
        Some(ScalarType::Str | ScalarType::String | ScalarType::CowStr) => {
            let s = peek
                .as_str()
                .ok_or_else(|| serde::ser::Error::custom("expected string value"))?;
            serializer.serialize_str(s)
        }
        _ => {
            // Fallback: use Display if available
            let s = peek.to_string();
            serializer.serialize_str(&s)
        }
    }
}

fn serialize_struct<'mem, 'facet, S: Serializer>(
    peek: Peek<'mem, 'facet>,
    kind: StructKind,
    shape: &'static Shape,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let ps = peek.into_struct().map_err(serde::ser::Error::custom)?;
    let name = shape.effective_name();

    match kind {
        StructKind::Unit => serializer.serialize_unit_struct(name),
        StructKind::TupleStruct => {
            let fields: Vec<_> = ps.fields_for_serialize().collect();
            let mut state = serializer.serialize_tuple_struct(name, fields.len())?;
            for (_, peek) in &fields {
                state.serialize_field(&PeekSerialize(*peek))?;
            }
            state.end()
        }
        StructKind::Tuple => {
            let fields: Vec<_> = ps.fields_for_serialize().collect();
            let mut state = serializer.serialize_tuple_struct("", fields.len())?;
            for (_, peek) in &fields {
                state.serialize_field(&PeekSerialize(*peek))?;
            }
            state.end()
        }
        StructKind::Struct => {
            // Use SerializeMap to avoid the &'static str requirement of SerializeStruct
            let mut state = serializer.serialize_map(None)?;
            for (field_item, peek) in ps.fields_for_serialize() {
                let field_name = field_item.effective_name();
                state.serialize_entry(field_name, &PeekSerialize(peek))?;
            }
            state.end()
        }
    }
}

fn serialize_enum<'mem, 'facet, S: Serializer>(
    pe: PeekEnum<'mem, 'facet>,
    shape: &'static Shape,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let variant_idx = pe.variant_index().map_err(serde::ser::Error::custom)?;
    let variant = pe.active_variant().map_err(serde::ser::Error::custom)?;
    let variant_name = variant.effective_name();
    let enum_name = shape.effective_name();

    let fields: Vec<_> = pe.fields_for_serialize().collect();

    match variant.data.kind {
        StructKind::Unit => {
            serializer.serialize_unit_variant(enum_name, variant_idx as u32, variant_name)
        }
        StructKind::TupleStruct if fields.len() == 1 => {
            // Newtype variant
            serializer.serialize_newtype_variant(
                enum_name,
                variant_idx as u32,
                variant_name,
                &PeekSerialize(fields[0].1),
            )
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            let mut state = serializer.serialize_tuple_variant(
                enum_name,
                variant_idx as u32,
                variant_name,
                fields.len(),
            )?;
            for (_, peek) in &fields {
                state.serialize_field(&PeekSerialize(*peek))?;
            }
            state.end()
        }
        StructKind::Struct => {
            // Use a map-based serialization to avoid &'static str requirement
            // Serialize as externally tagged: { "VariantName": { field: value, ... } }
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_key(variant_name)?;
            // Inner struct as a map
            let inner = StructVariantMapValue { fields: &fields };
            map.serialize_value(&inner)?;
            map.end()
        }
    }
}

struct StructVariantMapValue<'a, 'mem, 'facet> {
    fields: &'a [(facet_reflect::FieldItem, Peek<'mem, 'facet>)],
}

impl<'a, 'mem, 'facet> Serialize for StructVariantMapValue<'a, 'mem, 'facet> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_map(Some(self.fields.len()))?;
        for (field_item, peek) in self.fields {
            let field_name = field_item.effective_name();
            state.serialize_entry(field_name, &PeekSerialize(*peek))?;
        }
        state.end()
    }
}

fn serialize_seq<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let list = peek.into_list_like().map_err(serde::ser::Error::custom)?;
    let mut state = serializer.serialize_seq(Some(list.len()))?;
    for item in list.iter() {
        state.serialize_element(&PeekSerialize(item))?;
    }
    state.end()
}

fn serialize_set<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let set = peek.into_set().map_err(serde::ser::Error::custom)?;
    let mut state = serializer.serialize_seq(Some(set.len()))?;
    for item in set.iter() {
        state.serialize_element(&PeekSerialize(item))?;
    }
    state.end()
}

fn serialize_map<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let map = peek.into_map().map_err(serde::ser::Error::custom)?;
    let mut state = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map.iter() {
        state.serialize_entry(&PeekSerialize(k), &PeekSerialize(v))?;
    }
    state.end()
}

fn serialize_option<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let opt = peek.into_option().map_err(serde::ser::Error::custom)?;
    match opt.value() {
        Some(inner) => serializer.serialize_some(&PeekSerialize(inner)),
        None => serializer.serialize_none(),
    }
}

fn serialize_result<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    // Serialize as externally tagged enum: {"Ok": value} or {"Err": value}
    let pe = peek.into_enum().map_err(serde::ser::Error::custom)?;
    let variant_idx = pe.variant_index().map_err(serde::ser::Error::custom)?;
    let variant = pe.active_variant().map_err(serde::ser::Error::custom)?;
    let variant_name = variant.effective_name();

    let fields: Vec<_> = pe.fields_for_serialize().collect();
    if fields.len() == 1 {
        serializer.serialize_newtype_variant(
            "Result",
            variant_idx as u32,
            variant_name,
            &PeekSerialize(fields[0].1),
        )
    } else {
        serializer.serialize_unit_variant("Result", variant_idx as u32, variant_name)
    }
}

fn serialize_pointer<S: Serializer>(peek: Peek<'_, '_>, serializer: S) -> Result<S::Ok, S::Error> {
    let ptr = peek.into_pointer().map_err(serde::ser::Error::custom)?;
    let inner = ptr
        .borrow_inner()
        .ok_or_else(|| serde::ser::Error::custom("cannot borrow pointer inner value"))?;
    serialize_peek(inner, serializer)
}
