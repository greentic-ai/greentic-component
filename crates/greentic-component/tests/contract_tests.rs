mod contract;

#[test]
fn contract_suite_runs_for_component_world() {
    for world in contract::registry() {
        let wasm_path = world.fixture_dir.join("component.wasm");
        let manifest_path = world.fixture_dir.join("component.manifest.json");
        if !wasm_path.exists() || !manifest_path.exists() {
            eprintln!(
                "contract fixtures missing for {}, skipping",
                world.fixture_dir.display()
            );
            continue;
        }
        contract::run_contract_suite(&world);
    }
}

#[cfg(feature = "fuzz")]
mod fuzz {
    use super::contract;
    use proptest::prelude::*;
    use serde_json::{Number, Value};

    fn json_strategy() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|value| Value::Number(Number::from(value))),
            ".*".prop_map(Value::String),
        ];
        leaf.prop_recursive(3, 64, 10, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                prop::collection::hash_map(".*", inner, 0..4)
                    .prop_map(|map| { Value::Object(map.into_iter().collect()) }),
            ]
        })
    }

    #[test]
    fn contract_fuzz_component_world() {
        if std::env::var("GREENTIC_FUZZ").ok().as_deref() != Some("1") {
            eprintln!("GREENTIC_FUZZ not set; skipping fuzz suite");
            return;
        }
        let world = match contract::registry().into_iter().next() {
            Some(world) => world,
            None => return,
        };
        let wasm_path = world.fixture_dir.join("component.wasm");
        let manifest_path = world.fixture_dir.join("component.manifest.json");
        if !wasm_path.exists() || !manifest_path.exists() {
            eprintln!(
                "contract fixtures missing for {}, skipping",
                world.fixture_dir.display()
            );
            return;
        }

        proptest!(|(input in json_strategy())| {
            let output = contract::run_harness_once(&world, &input);
            prop_assert!(output.get("status").is_some());
        });
    }
}
