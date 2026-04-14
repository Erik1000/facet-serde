use facet::Facet;
use facet_reflect::Peek;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Adapter, ser::PeekSerialize};

/// Utilty function for use with serde(with = "module")
pub fn deserialize<'de, D, T>(de: D) -> Result<T, D::Error>
where
    T: Facet<'static>,
    D: Deserializer<'de>,
{
    Adapter::deserialize(de).map(|i| i.0)
}

/// Utility function for use wit serde(with = "module")
pub fn serialize<'f, S, T>(value: &T, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Facet<'f>,
{
    let peek = Peek::new(value);
    PeekSerialize(peek).serialize(ser)
}
