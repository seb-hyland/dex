//! Stubs are rendered at runtime from the live bindings, so they cannot go
//! stale. What they still need is completeness: anything reachable from a
//! script must be described, or an editor silently reports it as unknown.

use pyo3::prelude::*;

/// Anything a script can reach must be described, or the editor silently
/// reports it as unknown.
#[test]
fn stubs_describe_everything_bound() {
    dex_nodes::scripting::init_python();

    let described = {
        let mut map: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            Default::default();
        for class in dex_core::stubs::classes() {
            let entry = map.entry(class.name.to_owned()).or_default();
            for f in class.fields {
                entry.insert(f.name.to_owned());
            }
            // A data-carrying enum's variants are subclasses; a unit-only
            // enum's are attributes. Either way they are reachable members.
            for v in class.variants {
                entry.insert(v.name.to_owned());
            }
        }
        for m in dex_core::stubs::methods() {
            map.entry(m.owner.to_owned())
                .or_default()
                .insert(m.name.to_owned());
        }
        map
    };

    let mut missing: Vec<String> = Vec::new();
    Python::attach(|py| {
        let module = dex_dynamic::build_python_module(py).unwrap();
        let names: Vec<String> = module.dir().unwrap().extract().unwrap();

        for name in names.iter().filter(|n| !n.starts_with('_')) {
            let Ok(cls) = module.getattr(name.as_str()) else {
                continue;
            };
            // Only classes carry members worth describing.
            if !cls.is_instance_of::<pyo3::types::PyType>() {
                continue;
            }
            let Some(described) = described.get(name) else {
                missing.push(format!("class {name}"));
                continue;
            };
            let members: Vec<String> = cls.dir().unwrap().extract().unwrap();
            for member in members.iter().filter(|m| !m.starts_with('_')) {
                if !described.contains(member) {
                    missing.push(format!("{name}.{member}"));
                }
            }
        }
    });

    assert!(
        missing.is_empty(),
        "bound but undescribed by the stubs: {missing:#?}"
    );
}
