//! The namespace a lambda runs in: what it costs, and who outranks whom.
//!
//! Three claims, and each one used to be false:
//!
//!   * The prelude runs **once** per lambda run. The argument checks and the
//!     script want the same namespace, and preparing it is most of what a run
//!     costs, so they share one rather than building it each.
//!   * A declaration that names nothing the prelude defines does not run it at
//!     all. `n > 0` is a question about an argument, and answering it used to
//!     cost several thousand lines of library first — which is what a lambda
//!     sitting on a guard that says "stop" paid on every poll.
//!   * The prelude has a namespace of its own. Its exports are copied into the
//!     script's, so a bare `line(...)` still works, but an argument of the same
//!     name outranks the export without reaching inside the library: the
//!     prelude's own functions go on resolving each other in their own module.
//!     `dex.prelude` is that module, for naming a shadowed one on purpose.

use dex_core::prelude::*;
use dex_core::snapshot::GraphSnapshot;
use dex_nodes::argtypes::{ArgSpec, ArgType, check_arg_types_in};
use dex_nodes::primitives::text::Label;
use dex_nodes::scripting::{ScriptArg, ScriptEnv, ScriptOutput, ScriptValue};

/**
    A prelude that counts how many times it has been run.

    The count goes on `sys`, which outlives any one namespace — the point is to
    see runs that happen in *different* namespaces, so nothing local would do.
    It is keyed per test, because the tests in this binary share an interpreter
    and run at the same time, and a shared counter would read every one of them.

    `card` calling `line` is the shadowing case: a script that binds `line` must
    not change what `card` does.
*/
fn counting_prelude(key: &str) -> String {
    format!(
        r#"
import sys
sys.{key} = getattr(sys, "{key}", 0) + 1

MARKER = 7

def line(a, b):
    return "prelude line"

def card(a, b):
    return line(a, b) + " in a card"
"#
    )
}

/// How many times this test's prelude has been run.
fn prelude_runs(key: &str) -> i64 {
    pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        py.import("sys")
            .and_then(|sys| sys.getattr(key))
            .and_then(|v| v.extract::<i64>())
            .unwrap_or(0)
    })
}

/// One argument called `x`, declared and carrying `value`.
fn spec(kind: ArgType, detail: &str, value: ScriptValue) -> ArgSpec {
    ArgSpec {
        name: "x".to_owned(),
        port: NodeUid::<Label>::mint().erase(),
        kind,
        detail: detail.to_owned(),
        value: Some(value),
        source: None,
    }
}

/// The same argument, as the script will be handed it.
fn args_of(specs: &[ArgSpec]) -> Vec<ScriptArg> {
    specs.iter().filter_map(ArgSpec::as_script_arg).collect()
}

/// Check `specs` and then run `source`, in one namespace, as a lambda does.
///
/// Returns the faults, whatever the script produced, and how many times the
/// prelude was run getting there.
fn check_then_run(
    key: &str,
    specs: &[ArgSpec],
    source: Option<&str>,
) -> (Vec<String>, Option<String>, i64) {
    dex_nodes::scripting::init_python();
    let prelude = counting_prelude(key);
    let ws = Workspace::new_empty();
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let graph = GraphSnapshot::capture(&ws);
    let args = args_of(specs);

    let before = prelude_runs(key);
    let result = pyo3::Python::attach(|py| {
        let mut env = ScriptEnv::new(py, &prelude)
            .and_then(|env| env.with_runtime(&handle, graph))
            .and_then(|env| env.with_args(&args))
            .expect("the namespace is built");
        let faults: Vec<String> = check_arg_types_in(&mut env, specs)
            .into_iter()
            .map(|f| f.message)
            .collect();
        let produced = match (faults.is_empty(), source) {
            (true, Some(source)) => match env.run(source) {
                // A script returning a string hands back the label it becomes.
                Ok(ScriptOutput::Node(node)) => Some(
                    node.as_ref()
                        .as_any_ref()
                        .downcast_ref::<Label>()
                        .map(|l| l.text.clone())
                        .unwrap_or_default(),
                ),
                Ok(_) => Some(String::new()),
                Err(e) => Some(format!("error: {e}")),
            },
            _ => None,
        };
        (faults, produced)
    });
    let (faults, produced) = result;
    (faults, produced, prelude_runs(key) - before)
}

/**
    A declaration about its own argument is settled without the library.

    This is the state a settled machine sits in: every lambda in it is holding a
    guard that has refused, and re-checking that refusal is all the work there
    is. It has to cost nothing, or "at rest" is not rest.
*/
#[test]
fn a_declaration_that_names_nothing_from_the_prelude_does_not_run_it() {
    // Refused, so no script follows it either — the whole poll is one `eval`.
    let (faults, _, runs) = check_then_run(
        "runs_unneeded",
        &[spec(ArgType::Satisfying, "x > 0", ScriptValue::Int(0))],
        Some("def transform():\n    return 1\n"),
    );
    assert_eq!(faults.len(), 1, "`x > 0` refuses a zero: {faults:?}");
    assert_eq!(runs, 0, "the prelude should not have run at all");
}

/// A declaration that names the library does run it, or it could not be settled.
#[test]
fn a_declaration_that_names_the_prelude_runs_it() {
    let (faults, _, runs) = check_then_run(
        "runs_needed",
        &[spec(ArgType::Satisfying, "x > MARKER", ScriptValue::Int(9))],
        None,
    );
    assert!(faults.is_empty(), "9 > 7, so the declaration holds: {faults:?}");
    assert_eq!(runs, 1, "the prelude had to run for `MARKER` to resolve");
}

/**
    A check and the script it guards share one prelude.

    Both halves want the same namespace, and each used to build its own — so a
    lambda whose arguments were declared paid for the library twice on every
    single run.
*/
#[test]
fn a_check_and_the_script_it_guards_share_one_prelude() {
    let (faults, produced, runs) = check_then_run(
        "runs_shared",
        &[spec(ArgType::Satisfying, "x > MARKER", ScriptValue::Int(9))],
        Some("def transform():\n    return card(1, 2)\n"),
    );
    assert!(faults.is_empty(), "the declaration holds: {faults:?}");
    assert!(produced.is_some(), "the script ran");
    assert_eq!(runs, 1, "one run of the prelude, not one each");
}

/**
    An argument outranks a prelude name without reaching inside the library.

    The prelude and the script used to share one namespace, so an argument
    called `line` — a plausible name, and one of the library's own drawing
    helpers — was silently replaced by the function before the script saw it.
    Now the argument wins where the script can see it, and `card`, which calls
    `line`, goes on getting the library's.
*/
#[test]
fn an_argument_outranks_a_prelude_name_without_reaching_inside_it() {
    let specs = [
        spec(ArgType::Any, "", ScriptValue::Str("mine".to_owned())),
        ArgSpec {
            name: "line".to_owned(),
            ..spec(ArgType::Any, "", ScriptValue::Str("mine".to_owned()))
        },
    ];
    let (faults, produced, _) = check_then_run(
        "runs_shadow",
        &specs,
        // `line` is the argument; the `line` inside `card` is the library's.
        Some("def transform():\n    return line + \"|\" + card(1, 2)\n"),
    );
    assert!(faults.is_empty(), "nothing was declared: {faults:?}");
    let produced = produced.expect("the script ran");
    assert!(
        produced.contains("mine"),
        "the argument wins in the script's own namespace: {produced}"
    );
    assert!(
        produced.contains("prelude line in a card"),
        "the library still resolves its own names: {produced}"
    );
}

/// `dex.prelude` names the library's own, whatever the script has bound.
#[test]
fn dex_prelude_reaches_a_name_something_nearer_has_shadowed() {
    let specs = [ArgSpec {
        name: "line".to_owned(),
        ..spec(ArgType::Any, "", ScriptValue::Str("mine".to_owned()))
    }];
    let (faults, produced, _) = check_then_run(
        "runs_escape",
        &specs,
        Some("def transform():\n    return dex.prelude.line(1, 2)\n"),
    );
    assert!(faults.is_empty(), "nothing was declared: {faults:?}");
    let produced = produced.expect("the script ran");
    assert!(
        produced.contains("prelude line"),
        "`dex.prelude` reaches past the argument: {produced}"
    );
}
