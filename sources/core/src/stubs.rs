//! Type metadata for generating Python stubs (`.pyi`).

use std::collections::BTreeSet;

/// One named, typed slot: a struct field or a method parameter.
pub struct StubField {
    pub name: &'static str,
    /// The Rust type, as written. Mapped to Python by [`python_type`].
    pub ty: &'static str,
}

/// One variant of a bound enum.
pub struct StubVariant {
    pub name: &'static str,
    /// Empty for a unit variant; tuple fields are named `_0`, `_1`, ... as pyo3 names them.
    pub fields: &'static [StubField],
}

/// A bound class.
pub struct StubClass {
    pub name: &'static str,
    pub doc: &'static str,
    pub fields: &'static [StubField],
    /// Whether a script may call the class to construct one.
    pub constructible: bool,
    /// Non-empty for an enum. pyo3 exposes a data-carrying enum's variants as
    /// subclasses and a unit-only enum's as class attributes.
    pub variants: &'static [StubVariant],
}

/// A method or associated function on a bound class.
pub struct StubMethod {
    /// The script-facing name of the owning class.
    pub owner: &'static str,
    pub name: &'static str,
    pub doc: &'static str,
    pub params: &'static [StubField],
    /// The Rust return type, or `""` for unit.
    pub returns: &'static str,
    pub is_static: bool,
}

/// Marks a bound class as a node type, so the stub can have it subclass `Node`.
pub struct StubNodeImpl {
    pub name: &'static str,
}

dex_dynamic::__rt::inventory::collect!(StubNodeImpl);

/// The names of every bound class that is a node.
pub fn node_types() -> BTreeSet<&'static str> {
    dex_dynamic::__rt::inventory::iter::<StubNodeImpl>
        .into_iter()
        .map(|n| n.name)
        .collect()
}

dex_dynamic::__rt::inventory::collect!(StubClass);
dex_dynamic::__rt::inventory::collect!(StubMethod);

/// Every registered class, sorted by name.
pub fn classes() -> Vec<&'static StubClass> {
    let mut all: Vec<_> = dex_dynamic::__rt::inventory::iter::<StubClass>
        .into_iter()
        .collect();
    all.sort_by_key(|c| c.name);
    all
}

/// Every registered method, sorted by owner then name.
pub fn methods() -> Vec<&'static StubMethod> {
    let mut all: Vec<_> = dex_dynamic::__rt::inventory::iter::<StubMethod>
        .into_iter()
        .collect();
    all.sort_by_key(|m| (m.owner, m.name));
    all
}

// ======================================================================
// Rust type -> Python type
// ======================================================================

/**
    Render a Rust type as its Python annotation.

    `known` is the set of bound class names; a path type outside it cannot be
    described, so it degrades to `Any` rather than naming something a script
    cannot import.
*/
pub fn python_type(rust: &str, known: &BTreeSet<&str>) -> String {
    python_type_at(rust, known, Position::Output)
}

/// Where a type appears, which changes how a node is described: a parameter
/// takes anything coercible to one, a return hands back an opaque node.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Input,
    Output,
}

pub fn python_type_at(rust: &str, known: &BTreeSet<&str>, pos: Position) -> String {
    let normalised: String = rust.chars().filter(|c| !c.is_whitespace()).collect();
    render(&normalised, known, pos)
}

fn render(ty: &str, known: &BTreeSet<&str>, pos: Position) -> String {
    let ty = ty.trim_start_matches('&').trim();
    if ty.is_empty() {
        return "None".to_owned();
    }

    // A generic wrapper: split head from its arguments.
    if let Some((head, args)) = split_generic(ty) {
        let parts = split_args(args);
        return match head {
            "Option" => format!("{} | None", render(&parts[0], known, pos)),
            "Vec" | "VecDeque" => format!("list[{}]", render(&parts[0], known, pos)),
            "HashMap" | "BTreeMap" if parts.len() == 2 => format!(
                "dict[{}, {}]",
                render(&parts[0], known, pos),
                render(&parts[1], known, pos)
            ),
            // A node id carries only a compile-time tag; scripts see one type.
            "NodeUid" => "NodeUid".to_owned(),
            // Any value can stand in for a node.
            "Arc" | "Box" | "Rc" => render(&parts[0], known, pos),
            _ => leaf(head, known, pos),
        };
    }

    // A tuple.
    if ty.starts_with('(') && ty.ends_with(')') {
        let inner = &ty[1..ty.len() - 1];
        if inner.is_empty() {
            return "None".to_owned();
        }
        let parts: Vec<String> = split_args(inner)
            .iter()
            .map(|p| render(p, known, pos))
            .collect();
        return format!("tuple[{}]", parts.join(", "));
    }

    leaf(ty, known, pos)
}

fn leaf(ty: &str, known: &BTreeSet<&str>, pos: Position) -> String {
    // Take the last path segment: `crate::Vector` is `Vector`.
    let ty = ty.rsplit("::").next().unwrap_or(ty);
    match ty {
        "f32" | "f64" => "float".to_owned(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => "int".to_owned(),
        "bool" => "bool".to_owned(),
        "String" | "str" | "Cow<'static,str>" => "str".to_owned(),
        "" | "()" => "None".to_owned(),
        // The erased node trait object: any value a script can turn into a node.
        "dynNode" | "Node" => match pos {
            // `to_dyn_node_py` turns a str/int/float/bool/None into a node too.
            Position::Input => "NodeLike".to_owned(),
            Position::Output => "Node".to_owned(),
        },
        "NodeUid" | "NodeHandle" => "NodeUid".to_owned(),
        // A layout child accepts a live handle or a value.
        "LayoutChild" => match pos {
            Position::Input => "NodeUid | NodeLike".to_owned(),
            Position::Output => "NodeUid | Node".to_owned(),
        },
        // Not a class: crosses as `None | (at_start_of_line, continuation)`.
        "WrapConstraints" => "tuple[bool, int | None] | None".to_owned(),
        "RecordBatch" => "Any".to_owned(),
        other if known.contains(other) => other.to_owned(),
        _ => "Any".to_owned(),
    }
}

/// Split `Head<args>` into its head and argument text.
fn split_generic(ty: &str) -> Option<(&str, &str)> {
    let open = ty.find('<')?;
    if !ty.ends_with('>') {
        return None;
    }
    Some((&ty[..open], &ty[open + 1..ty.len() - 1]))
}

/// Split generic or tuple arguments on top-level commas.
fn split_args(args: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in args.chars() {
        match c {
            '<' | '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            '>' | ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> BTreeSet<&'static str> {
        ["Vector", "Label", "ScreenRegion"].into_iter().collect()
    }

    #[test]
    fn maps_primitives_and_wrappers() {
        let k = known();
        assert_eq!(python_type("f32", &k), "float");
        assert_eq!(python_type("usize", &k), "int");
        assert_eq!(python_type("bool", &k), "bool");
        assert_eq!(python_type("String", &k), "str");
        assert_eq!(python_type("", &k), "None");
        assert_eq!(python_type("Option < f32 >", &k), "float | None");
        assert_eq!(python_type("Vec < String >", &k), "list[str]");
        assert_eq!(
            python_type("Vec < (String, Option < NodeUid >) >", &k),
            "list[tuple[str, NodeUid | None]]"
        );
    }

    #[test]
    fn maps_node_shapes() {
        let k = known();
        assert_eq!(python_type("NodeUid", &k), "NodeUid");
        assert_eq!(python_type("NodeUid < Canvas >", &k), "NodeUid");
        assert_eq!(python_type("Arc < dyn Node >", &k), "Node");
        assert_eq!(python_type("LayoutChild", &k), "NodeUid | Node");
        // A parameter takes anything coercible; a return is opaque.
        assert_eq!(
            python_type_at("Arc < dyn Node >", &k, Position::Input),
            "NodeLike"
        );
    }

    #[test]
    fn known_classes_pass_through_and_others_degrade() {
        let k = known();
        assert_eq!(python_type("Vector", &k), "Vector");
        assert_eq!(python_type("Option < Label >", &k), "Label | None");
        // Nothing a script can name, so do not pretend.
        assert_eq!(python_type("IncrementalTypstWorld", &k), "Any");
    }
}
