//! Declaring what a lambda's argument is for, and what happens when the wire
//! carries something else.

use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::argtypes::{ArgSpec, ArgType, check_arg_types};
use dex_nodes::composites::lambda::{
    AddArg, ArgDeclaration, ArgInput, ArgKindPicker, ArgNodes, Lambda, LambdaArgsNode,
    LambdaOutput, PortFaulty, SetConnection,
};
use dex_nodes::layouts::desktops::Desktops;
use dex_nodes::layouts::error::ErrorLayout;
use dex_nodes::primitives::dropdown::{Dropdown, DropdownSelection, SetDropdownSelection};
use dex_nodes::primitives::number::Integer;
use dex_nodes::primitives::text::{GetText, Label, LabelEditable};
use dex_nodes::scripting::ScriptValue;

/// One argument called `x`, carried by an anonymous port.
fn spec(kind: ArgType, detail: &str, value: ScriptValue) -> ArgSpec {
    ArgSpec {
        name: "x".to_owned(),
        port: NodeUid::<Label>::mint().erase(),
        kind,
        detail: detail.to_owned(),
        value: Some(value),
    }
}

/// The same, with nothing on its port.
fn unwired(kind: ArgType, detail: &str) -> ArgSpec {
    ArgSpec {
        value: None,
        ..spec(kind, detail, ScriptValue::Nothing)
    }
}

#[test]
fn a_declaration_is_kept_or_complained_about() {
    dex_nodes::scripting::init_python();

    let cases: [(ArgType, &str, ScriptValue, bool); 9] = [
        // Anything at all goes anywhere.
        (ArgType::Any, "", ScriptValue::Nothing, true),
        (ArgType::Text, "", ScriptValue::Str("hi".to_owned()), true),
        (ArgType::Text, "", ScriptValue::Int(3), false),
        (ArgType::Int, "", ScriptValue::Int(3), true),
        // A whole number is a float; a float is not a whole number.
        (ArgType::Float, "", ScriptValue::Int(3), true),
        (ArgType::Int, "", ScriptValue::Float(3.0), false),
        (ArgType::Bool, "", ScriptValue::Bool(true), true),
        (ArgType::Bool, "", ScriptValue::Int(1), false),
        (ArgType::Text, "", ScriptValue::Nothing, false),
    ];

    for (kind, detail, value, ok) in cases {
        let faults = check_arg_types("", &[spec(kind, detail, value.clone())]);
        assert_eq!(
            faults.is_empty(),
            ok,
            "{kind:?} against {value:?} should {}",
            if ok { "pass" } else { "fail" }
        );
    }
}

/**
    An argument with nothing on it is reported as that, not as a Python error.

    Which is the whole point of saying it: an unwired argument binds no name, so
    what the script would otherwise raise is a `NameError` about an identifier
    the reader never wrote, four lines into a script that is not the problem.
*/
#[test]
fn an_unwired_argument_says_it_is_unwired() {
    dex_nodes::scripting::init_python();

    let faults = check_arg_types("", &[unwired(ArgType::Table, "")]);
    assert_eq!(faults.len(), 1, "one argument, one complaint");
    assert!(
        faults[0].message.contains("nothing is wired to it"),
        "and it says so plainly: {}",
        faults[0].message
    );
    assert!(
        faults[0].message.contains("a table"),
        "naming what it was asked for: {}",
        faults[0].message
    );

    assert!(
        check_arg_types("", &[unwired(ArgType::Any, "")]).is_empty(),
        "an argument that asks for nothing in particular is content with nothing"
    );
    // The interpreter is never reached for these: there is no value to test,
    // and the expression would only fail on the name that was never bound.
    let written = check_arg_types("", &[unwired(ArgType::Satisfying, "x > 3")]);
    assert_eq!(written.len(), 1);
    assert!(
        written[0].message.contains("nothing is wired to it"),
        "including the written kinds: {}",
        written[0].message
    );
}

/// What arrived is named in the same words the declaration is offered in.
#[test]
fn the_complaint_names_what_actually_arrived() {
    dex_nodes::scripting::init_python();

    for (value, expected) in [
        (ScriptValue::Str("hi".to_owned()), "text"),
        (ScriptValue::Int(1), "an integer"),
        (ScriptValue::Float(1.0), "a float"),
        (ScriptValue::Bool(true), "a boolean"),
    ] {
        // Asked for something it is not, so the complaint has to name it.
        let wanted = if expected == "text" {
            ArgType::Int
        } else {
            ArgType::Text
        };
        let faults = check_arg_types("", &[spec(wanted, "", value.clone())]);
        assert_eq!(faults.len(), 1, "{value:?} is not {}", wanted.label());
        assert!(
            faults[0].message.ends_with(&format!("and got {expected}.")),
            "in words, not in annotations: {}",
            faults[0].message
        );
    }
}

/// The two written kinds go through the interpreter, with the prelude in scope.
#[test]
fn a_written_declaration_is_settled_by_the_interpreter() {
    dex_nodes::scripting::init_python();

    let named = |detail, value| check_arg_types("", &[spec(ArgType::Instance, detail, value)]);
    assert!(named("str", ScriptValue::Str("hi".to_owned())).is_empty());
    assert!(!named("int", ScriptValue::Str("hi".to_owned())).is_empty());

    // A name the prelude defines resolves, which is the point of running it.
    const PRELUDE: &str = "class Tiny(int):\n    pass\n";
    assert!(
        check_arg_types(
            PRELUDE,
            &[spec(ArgType::Instance, "Tiny | int", ScriptValue::Int(2))]
        )
        .is_empty(),
        "a type the prelude introduced is in scope for the check"
    );

    let satisfying =
        |detail, value| check_arg_types("", &[spec(ArgType::Satisfying, detail, value)]);
    assert!(satisfying("x > 3", ScriptValue::Int(5)).is_empty());
    assert!(!satisfying("x > 3", ScriptValue::Int(1)).is_empty());

    // An expression that will not run is itself the complaint.
    let broken = satisfying("x.no_such_method()", ScriptValue::Int(1));
    assert_eq!(broken.len(), 1, "one argument, one complaint");
    assert!(
        broken[0].message.contains('x'),
        "the complaint names the argument: {}",
        broken[0].message
    );

    // Nothing written yet is nothing being asked for, not a failure.
    assert!(
        named("", ScriptValue::Int(1)).is_empty(),
        "an unfinished declaration asks for nothing"
    );
}

/// Tick and drain for up to `rounds` turns, or until `done`.
fn settle_within(
    ws: &mut Workspace,
    rounds: usize,
    mut done: impl FnMut(&Workspace) -> bool,
) -> bool {
    for _ in 0..rounds {
        ws.tick_all();
        ws.process_pending();
        if done(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/**
    Long enough for a worker to have answered.

    Generous on purpose: several tests in this binary each run a scheduler, and
    every one of their workers wants the same interpreter lock. A budget tight
    enough to be quick on an idle machine fails on a busy one, and a test that
    fails on a busy machine is worse than a slow one.
*/
fn settle(ws: &mut Workspace, done: impl FnMut(&Workspace) -> bool) -> bool {
    settle_within(ws, 2000, done)
}

/// Long enough to be sure nothing is going to happen.
fn stays(ws: &mut Workspace, done: impl FnMut(&Workspace) -> bool) -> bool {
    !settle_within(ws, 40, done)
}

/// A lambda with one argument, and a label and a number to wire to it.
struct Wired {
    ws: Workspace,
    port: NodeUid,
    picker: NodeUid,
    output: NodeUid,
    text: NodeUid,
    number: NodeUid,
}

fn one_argument() -> Wired {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let handle = ws.action_handle();
    let lambda = handle.insert_node(Lambda::new(handle.clone()));
    let text = handle
        .insert_node(Label::new("not a number".to_owned()))
        .erase();
    let number = handle.insert_node(Integer::new(7)).erase();
    ws.process_pending();

    let args = ws
        .send_request(lambda.erase(), LambdaArgsNode)
        .expect("the lambda has an argument row");
    ws.submit_action_dyn(Action {
        dest: args,
        description: "add an argument".into(),
        body: Box::new(AddArg),
    });
    ws.process_pending();

    let arg = *ws
        .send_request(args, ArgNodes)
        .unwrap_or_default()
        .first()
        .expect("the argument was added");
    let (_, port, _) = ws
        .send_request(arg, ArgInput)
        .expect("the argument has a port");
    let picker = ws
        .send_request(arg, ArgKindPicker)
        .expect("the argument declares a kind");
    let output = ws
        .send_request(lambda.erase(), LambdaOutput)
        .expect("the lambda has a result");

    // A first tick, so the lambda has seen the shape it starts in — exactly as
    // it would have on the frame before a hand reached for the wire.
    ws.tick_all();
    ws.process_pending();

    Wired {
        ws,
        port,
        picker,
        output,
        text,
        number,
    }
}

fn declare(w: &mut Wired, kind: ArgType) {
    w.ws.submit_action_dyn(Action {
        dest: w.picker,
        description: "declare the argument".into(),
        body: Box::new(SetDropdownSelection {
            index: kind.index(),
        }),
    });
    w.ws.process_pending();
}

fn wire(w: &mut Wired, target: Option<NodeUid>) {
    w.ws.submit_action_dyn(Action {
        dest: w.port,
        description: "wire the argument".into(),
        body: Box::new(SetConnection { target }),
    });
    w.ws.process_pending();
}

fn faulty(ws: &Workspace, port: NodeUid) -> bool {
    ws.send_request(port, PortFaulty).unwrap_or(false)
}

fn errored(ws: &Workspace, output: NodeUid) -> bool {
    ws.get_node(output)
        .is_some_and(|node| (*node).as_any_ref().is::<ErrorLayout>())
}

/// The wrong thing on the wire reddens the wire and stands in for the result.
#[test]
fn a_mistyped_argument_reddens_its_wire_and_replaces_the_result() {
    let mut w = one_argument();
    declare(&mut w, ArgType::Int);
    let text = w.text;
    wire(&mut w, Some(text));

    let port = w.port;
    assert!(
        settle(&mut w.ws, |ws| faulty(ws, port)),
        "the wire carrying text into an integer went red"
    );
    assert!(
        errored(&w.ws, w.output),
        "and the result says so instead of being computed"
    );
}

/// Rewiring to something that fits clears both.
#[test]
fn correcting_the_wire_clears_the_fault() {
    let mut w = one_argument();
    declare(&mut w, ArgType::Int);
    let text = w.text;
    wire(&mut w, Some(text));
    let port = w.port;
    assert!(
        settle(&mut w.ws, |ws| faulty(ws, port)),
        "it went red first"
    );

    let number = w.number;
    wire(&mut w, Some(number));
    assert!(
        settle(&mut w.ws, |ws| !faulty(ws, port)),
        "a whole number is what the argument asked for"
    );
    assert!(
        !errored(&w.ws, w.output),
        "and the result is computed again"
    );
}

/// Changing the declaration alone re-runs the lambda: nothing moved upstream,
/// but what the lambda *is* changed.
#[test]
fn changing_the_declaration_re_runs_the_check() {
    let mut w = one_argument();
    let text = w.text;
    wire(&mut w, Some(text));
    let port = w.port;
    // Undeclared, so nothing to complain about however long it is left.
    assert!(
        stays(&mut w.ws, |ws| faulty(ws, port)),
        "an undeclared argument accepts anything"
    );

    declare(&mut w, ArgType::Int);
    assert!(
        settle(&mut w.ws, |ws| faulty(ws, port)),
        "declaring a type re-checks what is already wired"
    );
}

// ================================================================================
// THE DECLARATION READS AS PART OF THE ROW
// ================================================================================

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

/// Draw one frame of `ws` into `ctx`, feeding it `events`.
fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let _ = ctx.clone().run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, screen);
            });
        },
    );
}

/// Draw `ws` a few times — the first pass over a new layout is a sizing pass —
/// and give back the context, which is holding where everything landed.
fn drawn(ws: &mut Workspace) -> egui::Context {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    for _ in 0..3 {
        frame(ws, &ctx, vec![]);
    }
    ctx
}

/// Press at `pos` and let the workspace settle.
fn click(ws: &mut Workspace, ctx: &egui::Context, pos: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(ws, ctx, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, vec![button(true)]);
    frame(ws, ctx, vec![button(false)]);
    for _ in 0..2 {
        ws.process_pending();
        frame(ws, ctx, vec![]);
    }
}

/**
    A lambda with one argument, drawn as the root of its own workspace.

    Not on a desktop: a canvas draws its items on a layer carrying a pan-and-zoom
    transform, so what egui records for them is in that layer's coordinates and
    a press addressed to the screen lands somewhere else entirely. Drawn at the
    root there is no transform to account for, and the row is the same row.
*/
fn drawn_argument() -> (Workspace, egui::Context, NodeUid, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let lambda = handle.insert_node(Lambda::new(handle.clone())).erase();
    ws.process_pending();
    ws.set_root(lambda);

    let args = ws
        .send_request(lambda, LambdaArgsNode)
        .expect("the lambda has an argument row");
    ws.submit_action_dyn(Action {
        dest: args,
        description: "add an argument".into(),
        body: Box::new(AddArg),
    });
    ws.process_pending();

    let arg = *ws
        .send_request(args, ArgNodes)
        .unwrap_or_default()
        .first()
        .expect("the argument was added");
    let picker = ws
        .send_request(arg, ArgKindPicker)
        .expect("the argument declares a kind");
    // The name field is the editable label holding the argument's own name.
    let (name, _, _, _) = ws
        .send_request(arg, ArgDeclaration)
        .expect("the argument has a declaration");
    let name_field = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<LabelEditable>())
                && ws.send_request(*uid, GetText).as_deref() == Some(name.as_str())
        })
        .expect("the name field is live");

    let ctx = drawn(&mut ws);
    (ws, ctx, picker, name_field)
}

/// Where a node drew, as egui recorded it.
fn rect_of(ctx: &egui::Context, uid: NodeUid) -> egui::Rect {
    ctx.read_response(egui::Id::new(uid))
        .unwrap_or_else(|| panic!("{uid:?} drew this frame"))
        .rect
}

/**
    The type sits on the same line as the name, not below it.

    An unboxed dropdown is a word in a sentence. Padding it like a control drops
    it off the baseline of the labels either side, which is exactly what it did
    before it was told it was a word.
*/
#[test]
fn the_declaration_sits_on_the_argument_s_own_line() {
    let (_ws, ctx, picker, name_field) = drawn_argument();
    let (declared, named) = (rect_of(&ctx, picker), rect_of(&ctx, name_field));

    assert!(
        (declared.top() - named.top()).abs() <= 1.0,
        "the type starts on the name's line: {} against {}",
        declared.top(),
        named.top()
    );
    assert!(
        (declared.height() - named.height()).abs() <= 1.0,
        "and stands exactly as tall: {} against {}",
        declared.height(),
        named.height()
    );
}

/// It is as wide as the word it is showing, not as wide as the longest word it
/// could show.
#[test]
fn the_declaration_is_only_as_wide_as_its_word() {
    let (mut ws, ctx, picker, _name) = drawn_argument();
    let narrow = rect_of(&ctx, picker).width();

    ws.submit_action_dyn(Action {
        dest: picker,
        description: "declare it".into(),
        body: Box::new(SetDropdownSelection {
            index: ArgType::Satisfying.index(),
        }),
    });
    ws.process_pending();
    let ctx = drawn(&mut ws);
    let wide = rect_of(&ctx, picker).width();

    assert!(
        wide > narrow + 8.0,
        "`{}` is wider than `{}`: {wide} against {narrow}",
        ArgType::Satisfying.label(),
        ArgType::Any.label(),
    );
}

/// And it is written in the same ink as the rest of the row.
#[test]
fn the_declaration_is_written_in_the_row_s_own_ink() {
    let (ws, _ctx, picker, _name) = drawn_argument();
    let picker = ws.get_node(picker).expect("the picker is live");
    let picker = (*picker)
        .as_any_ref()
        .downcast_ref::<Dropdown>()
        .expect("the kind is picked from a dropdown");

    let ink = dex_core::theme::INK;
    assert_eq!(
        (
            picker.color.r,
            picker.color.g,
            picker.color.b,
            picker.color.a
        ),
        (ink.r, ink.g, ink.b, ink.a),
        "the type is set in the same ink as the label and the name"
    );
    assert!(!picker.boxed, "and wears no frame of its own");
    assert_eq!(
        picker.font.size,
        dex_core::theme::TEXT_LG,
        "at the size an editable label comes up at, which is the row's size"
    );
}

/**
    The open list is a panel of rows, sized to the longest of them.

    The row that opens it is only as wide as the word it shows, which is right
    for a word in a sentence and nowhere near wide enough for a list of eight
    other words: `any satisfying` has to fit in the list even when `any` is what
    is showing.
*/
#[test]
fn the_open_list_fits_the_longest_option() {
    let (mut ws, ctx, picker, _name) = drawn_argument();
    let header = rect_of(&ctx, picker);
    click(&mut ws, &ctx, header.center());

    let rows: Vec<egui::Rect> = (0..dex_nodes::argtypes::ARG_TYPES.len())
        .map(|i| {
            ctx.read_response(egui::Id::new(picker).with(("dex_dropdown", i)))
                .unwrap_or_else(|| panic!("option {i} is showing"))
                .rect
        })
        .collect();

    let widest = rows.iter().map(|r| r.width()).fold(0.0f32, f32::max);
    let narrowest = rows.iter().map(|r| r.width()).fold(f32::MAX, f32::min);
    assert!(
        (widest - narrowest).abs() < 0.5,
        "every row is the same width: {narrowest} to {widest}"
    );
    assert!(
        narrowest > header.width() + 8.0,
        "and wider than the word that opened them: {narrowest} against {}",
        header.width()
    );
    assert!(
        rows[0].height() > header.height(),
        "a row in the list has room around its text, where the one in the \
         sentence has none: {} against {}",
        rows[0].height(),
        header.height()
    );

    // Stacked, in order, touching but never overlapping.
    for pair in rows.windows(2) {
        assert!(
            (pair[1].top() - pair[0].bottom()).abs() < 0.5,
            "one row starts where the last one ended: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
    // And clear of the row that opened them.
    assert!(
        rows[0].top() >= header.bottom(),
        "the list hangs below its own row"
    );
}

/// Choosing from the list takes the choice and puts the list away.
#[test]
fn choosing_from_the_list_closes_it() {
    let (mut ws, ctx, picker, _name) = drawn_argument();
    click(&mut ws, &ctx, rect_of(&ctx, picker).center());

    let table = ArgType::Table.index();
    let row = ctx
        .read_response(egui::Id::new(picker).with(("dex_dropdown", table)))
        .expect("the last option is showing")
        .rect;
    click(&mut ws, &ctx, row.center());

    assert_eq!(
        ws.send_request(picker.cast::<Dropdown>(), DropdownSelection),
        Some(table),
        "the option pressed is the one it now shows"
    );
    assert!(
        ctx.read_response(egui::Id::new(picker).with(("dex_dropdown", 0)))
            .is_none(),
        "and the list is put away"
    );
}
