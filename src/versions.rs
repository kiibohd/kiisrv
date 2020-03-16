use maplit::hashmap;
use std::collections::hash_map::HashMap;

pub fn version_map() -> HashMap<String, String> {
    hashmap! {
        "latest".to_string() => "controller-056".to_string(),
        "lts".to_string()    => "controller-050".to_string(),
        "v0.5.6".to_string() => "controller-056".to_string(),
        "v0.5.0".to_string() => "controller-050".to_string(),
    }
}
