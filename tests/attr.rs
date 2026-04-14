use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Facet, PartialEq)]
struct MyType {
    name: String,
    verified: bool,
}

#[derive(Debug, Serialize, Deserialize, Facet, PartialEq)]
struct SerdeType {
    #[serde(with = "facet_serde")]
    my: MyType,
    normal: u32,
}

#[test]
fn via_attribute() {
    let s = SerdeType {
        my: MyType {
            name: "sifsoif".to_string(),
            verified: true,
        },
        normal: 42,
    };
    let json = serde_json::to_string_pretty(&s).expect("valid json");
    let facet: SerdeType = facet_json::from_str(&json).expect("valid type in json");
    let serde: SerdeType = serde_json::from_str(&json).expect("valid type in json");
    assert_eq!(facet, serde, "both are the same")
}
