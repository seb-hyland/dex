//! What a lambda's argument will accept, and whether what is wired to it does.
//!
//! A lambda's arguments are untyped by default, which is right for building
//! something: you wire a thing up and find out what it does. It is wrong for
//! keeping something, where the first sign that a table went where a number
//! belonged is a Python traceback from four operators downstream. Declaring
//! what an argument is for moves that report back to the wire that carries it.
//!
//! The checks run on a worker thread, alongside the script rather than in front
//! of it: two of the eight need the interpreter (and the prelude, so a name the
//! prelude defines resolves), and the frame must not wait for either.

use dex_core::prelude::*;

use crate::scripting::{ScriptValue, seed_globals};

/// What an argument will accept, in the order the dropdown offers them.
#[derive(Copy, Debug, Default, PartialEq, Eq)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum ArgType {
    /// No claim: whatever is wired here is what the script gets.
    #[default]
    Any,
    /// An instance of a named Python type.
    Instance,
    /// Anything a written expression says yes to.
    Satisfying,
    Text,
    Int,
    Float,
    Bool,
    Table,
}

/// Every kind, in the order they are offered.
pub const ARG_TYPES: [ArgType; 8] = [
    ArgType::Any,
    ArgType::Instance,
    ArgType::Satisfying,
    ArgType::Text,
    ArgType::Int,
    ArgType::Float,
    ArgType::Bool,
    ArgType::Table,
];

#[utils::dynamic_methods]
impl ArgType {
    /// What the dropdown calls this kind. The two that take a written detail
    /// are named for the phrase they begin, since the field finishes it.
    pub fn label(&self) -> String {
        match self {
            ArgType::Any => "any",
            ArgType::Instance => "some",
            ArgType::Satisfying => "satisfying",
            ArgType::Text => "text",
            ArgType::Int => "an integer",
            ArgType::Float => "a float",
            ArgType::Bool => "a boolean",
            ArgType::Table => "a table",
        }
        .to_owned()
    }

    /// Whether this kind is finished by something the user writes.
    pub fn takes_detail(&self) -> bool {
        matches!(self, ArgType::Instance | ArgType::Satisfying)
    }

    /// The kind at `index` in the offered order, for a dropdown's answer.
    pub fn at(index: usize) -> ArgType {
        ARG_TYPES.get(index).copied().unwrap_or(ArgType::Any)
    }

    /// Where this kind sits in that order.
    pub fn index(&self) -> usize {
        ARG_TYPES.iter().position(|k| k == self).unwrap_or(0)
    }
}

impl ArgType {
    /// How this kind reads in a sentence about what was expected.
    fn wanted(&self, detail: &str) -> String {
        match self {
            ArgType::Any => "anything".to_owned(),
            ArgType::Instance => format!("some {detail}"),
            ArgType::Satisfying => format!("a value satisfying `{detail}`"),
            _ => self.label(),
        }
    }
}

/// Every label, for seeding the dropdown that picks one.
pub fn arg_type_labels() -> Vec<String> {
    ARG_TYPES.iter().map(ArgType::label).collect()
}

/// One argument, as the check sees it.
#[derive(Clone)]
pub struct ArgSpec {
    pub name: String,
    /// The port carrying it, which is what a fault is shown on.
    pub port: NodeUid,
    pub kind: ArgType,
    /// The type name or the expression, for the two kinds that take one.
    pub detail: String,
    /// What is wired to it, or [`None`] when nothing is.
    pub value: Option<ScriptValue>,
}

/// An argument that is not what it was asked to be.
#[derive(Clone)]
pub struct TypeFault {
    pub port: NodeUid,
    pub message: String,
}

/// The one line a lambda shows in place of its result when its arguments are wrong.
pub fn describe(faults: &[TypeFault]) -> String {
    faults
        .iter()
        .map(|fault| fault.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/**
    Whether `value` satisfies `kind`, as far as Rust can tell on its own.

    [`None`] for the two kinds that are finished by something written, which
    only the interpreter can settle.
*/
fn settled_here(kind: ArgType, value: &Option<ScriptValue>) -> Option<bool> {
    // Nothing on the wire settles every kind but the one that asks for nothing
    // in particular: there is no value to look at, and none is coming.
    let Some(value) = value else {
        return Some(kind == ArgType::Any);
    };
    Some(match kind {
        ArgType::Any => true,
        ArgType::Instance | ArgType::Satisfying => return None,
        ArgType::Text => matches!(value, ScriptValue::Str(_)),
        // A whole number is a float; a float is not a whole number. Widening
        // one way only, so wiring a count into something that divides is fine
        // and wiring a measurement into something that indexes is not.
        ArgType::Int => matches!(value, ScriptValue::Int(_)),
        ArgType::Float => matches!(value, ScriptValue::Float(_) | ScriptValue::Int(_)),
        ArgType::Bool => matches!(value, ScriptValue::Bool(_)),
        ArgType::Table => matches!(value, ScriptValue::Table(_)),
    })
}

/**
    What arrived, for the second half of the complaint.

    In the same words the declaration is offered in, so the two halves of the
    sentence are comparable. The Python annotation was what this used to say,
    and it named a table `typing.Any` — true of the stub and no use to anyone
    reading it.
*/
fn arrived(value: &Option<ScriptValue>) -> String {
    let Some(value) = value else {
        return "nothing is wired to it".to_owned();
    };
    match value {
        ScriptValue::Str(_) => ArgType::Text.label(),
        ScriptValue::Int(_) => ArgType::Int.label(),
        ScriptValue::Float(_) => ArgType::Float.label(),
        ScriptValue::Bool(_) => ArgType::Bool.label(),
        ScriptValue::Table(_) => ArgType::Table.label(),
        ScriptValue::Node(_) => "a node".to_owned(),
        ScriptValue::Nothing => "nothing".to_owned(),
    }
}

/// The complaint about one argument.
fn fault(spec: &ArgSpec, reason: Option<String>) -> TypeFault {
    let name = &spec.name;
    let want = spec.kind.wanted(&spec.detail);
    let message = match reason {
        Some(why) => format!("`{name}` was asked for {want}: {why}"),
        // An unwired argument is not the wrong kind of thing, it is no thing,
        // and saying so is the difference between a fixable report and a
        // Python error about a name that was never bound.
        None if spec.value.is_none() => {
            format!("`{name}` was asked for {want}, and nothing is wired to it.")
        }
        None => format!(
            "`{name}` was asked for {want}, and got {}.",
            arrived(&spec.value)
        ),
    };
    TypeFault {
        port: spec.port,
        message,
    }
}

/**
    Check every argument in `specs` against what it was asked to be.

    `prelude` is run first when the interpreter is needed at all, so a type or a
    helper the prelude defines is in scope for the two written kinds. Every
    argument is seeded, not only the one being checked, so an expression may
    speak about more than its own value.
*/
pub fn check_arg_types(prelude: &str, specs: &[ArgSpec]) -> Vec<TypeFault> {
    let mut faults = Vec::new();
    let mut deferred = Vec::new();
    for spec in specs {
        match settled_here(spec.kind, &spec.value) {
            Some(true) => {}
            Some(false) => faults.push(fault(spec, None)),
            None => deferred.push(spec),
        }
    }
    if !deferred.is_empty() {
        faults.extend(check_in_python(prelude, specs, &deferred));
    }
    faults
}

/// The two kinds the interpreter has to settle.
fn check_in_python(prelude: &str, all: &[ArgSpec], deferred: &[&ArgSpec]) -> Vec<TypeFault> {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use std::ffi::CString;

    let seeds: Vec<(String, ScriptValue)> = all
        .iter()
        .filter_map(|spec| Some((spec.name.clone(), spec.value.clone()?)))
        .collect();

    Python::attach(|py| {
        let globals = PyDict::new(py);
        let Ok(dex_mod) = dex_dynamic::build_python_module(py) else {
            return Vec::new();
        };
        if globals.set_item("__name__", "__main__").is_err()
            || globals.set_item("dex", &dex_mod).is_err()
            || seed_globals(py, &globals, &seeds).is_err()
        {
            return Vec::new();
        }
        // A prelude that will not run is the script's problem to report, not
        // this one's: running the script is what puts that error on screen.
        if let Ok(code) = CString::new(prelude) {
            let _ = py.run(code.as_c_str(), Some(&globals), Some(&globals));
        }

        deferred
            .iter()
            .filter_map(|spec| {
                // Nothing written yet is nothing being asked for: an argument
                // does not fail its declaration while it is still being made.
                if spec.detail.trim().is_empty() {
                    return None;
                }
                let source = match spec.kind {
                    ArgType::Instance => format!("isinstance({}, {})", spec.name, spec.detail),
                    _ => spec.detail.clone(),
                };
                let code = CString::new(source).ok()?;
                match py.eval(code.as_c_str(), Some(&globals), Some(&globals)) {
                    Ok(verdict) => match verdict.is_truthy() {
                        Ok(true) => None,
                        Ok(false) => Some(fault(spec, None)),
                        Err(e) => Some(fault(spec, Some(e.to_string()))),
                    },
                    // A test that will not run is itself the complaint: the
                    // argument cannot be shown to be what it was asked for.
                    Err(e) => Some(fault(spec, Some(e.to_string()))),
                }
            })
            .collect()
    })
}
