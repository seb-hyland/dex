//! The default prelude: the library every workspace opens with.
//!
//! It is compiled into the binary and run before every lambda, so it has to
//! parse, run, and answer for itself. These tests build views out of it the way
//! a transform would, draw them, and put the protocol's own questions to them.

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(760.0, 560.0);

/// Whether this interpreter can do Arrow at all.
///
/// The prelude requires pyarrow — it is how a table crosses into a script. The
/// repo keeps an environment with it under `demoenv/`, so a bare interpreter is
/// pointed at that rather than skipping every test that matters. `VIRTUAL_ENV`
/// cannot be used for this: pyo3 reads it while starting the interpreter and
/// then cannot find the standard library.
fn has_pyarrow() -> bool {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    dex_nodes::scripting::init_python();
    ONCE.call_once(|| {
        if pyo3::Python::attach(|py| py.import("pyarrow").is_ok()) {
            return;
        }
        let demoenv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demoenv/.venv");
        if demoenv.is_dir() {
            let _ = dex_nodes::settings::set_venv(Some(demoenv));
        }
    });
    pyo3::Python::attach(|py| py.import("pyarrow").is_ok())
}

/// Run `script` against the default prelude, seat what it returns as the root,
/// and hand back the workspace.
fn built(script: &str) -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args: [(String, ScriptValue); 0] = [];
    // A script may hand back the node itself, or the id of one it already
    // seated — a layout built with `build` does the latter.
    let output = match run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws)) {
        Ok(output) => output,
        Err(e) => panic!("{e}"),
    };
    let root = match output {
        ScriptOutput::Node(node) => ws.action_handle().insert_node_dyn(node),
        ScriptOutput::Handle(uid) => uid,
        ScriptOutput::Nothing => panic!("the script returns something"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(root);
    ws.process_pending();
    (ws, root)
}

/// One frame with the given events; returns how many shapes were painted.
fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) -> usize {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let out = ctx.clone().run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            ws.draw_frame(ui, screen);
        });
    });
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().map(count).sum(),
            _ => 1,
        }
    }
    out.shapes.iter().map(|c| count(&c.shape)).sum()
}

/// Draw twice — the first pass sizes, the second paints.
fn drawn(ws: &mut Workspace, ctx: &egui::Context) -> usize {
    let mut painted = 0;
    for _ in 0..2 {
        painted = frame(ws, ctx, Vec::new());
    }
    painted
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    ctx
}

/// Run a script of assertions against the prelude, and hand back its verdict.
///
/// The pure parts of the library — the statistics, the tree building — are
/// checked from Python rather than from here, because that is the language they
/// are written in and an assertion beside the code it is about reads better
/// than one that has to reach across the bridge to say the same thing.
fn checked(script: &str) -> String {
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let ws = Workspace::new_empty();
    let args: [(String, ScriptValue); 0] = [];
    let out = run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("every assertion holds");
    let ScriptOutput::Node(node) = out else {
        panic!("returns a verdict")
    };
    dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("a verdict")
}

/// Ask `target` something, through a script, and get the answer back as text.
fn ask(ws: &Workspace, target: NodeUid, expression: &str) -> String {
    let script = format!("def transform():\n    return str({expression})\n");
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(target))];
    let out = run_script(&script, PRELUDE, &handle, &args, GraphSnapshot::capture(ws))
        .expect("the asking script runs");
    let ScriptOutput::Node(node) = out else {
        panic!("the asker returns text")
    };
    dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("an answer")
}

/// The prelude parses and runs on its own, offering nothing but its library.
#[test]
fn the_prelude_runs() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let (_offers, error) = dex_nodes::prelude_prototypes::read(PRELUDE, &handle);
    assert_eq!(error, None, "the default prelude runs clean");
}

/// The statistics are pure and worth pinning: they are what every layout's
/// numbers come out of.
#[test]
fn the_statistics_hold() {
    dex_nodes::scripting::init_python();
    let _ = has_pyarrow();
    let checks = r#"
def transform():
    assert abs(pearson([(1, 1), (2, 2), (3, 3)]) - 1.0) < 1e-9
    assert abs(pearson([(1, 3), (2, 2), (3, 1)]) + 1.0) < 1e-9
    assert pearson([(1, 1)]) is None
    (a, b) = least_squares([(0, 1), (1, 3), (2, 5)])
    assert abs(a - 2.0) < 1e-9 and abs(b - 1.0) < 1e-9
    assert quartiles([1, 2, 3, 4, 5])[1] == 3
    assert quartiles([]) == (0.0, 0.0, 0.0)
    (lo, hi, step) = nice_bounds(0.3, 9.7)
    assert lo <= 0.3 and hi >= 9.7 and step > 0
    assert nice_bounds(5.0, 5.0)[0] < 5.0, "a flat range still spans something"
    assert tick_text(-0.0, 1.0) == "0"
    assert axis_ticks(0.0, 1.0, 0.25) == [0.0, 0.25, 0.5, 0.75, 1.0]
    assert cramers_v([[10, 0], [0, 10]], 2, 2, 20) > 0.9
    assert cramers_v([[5, 5], [5, 5]], 2, 2, 20) < 1e-9

    # A density integrates to about one, and peaks where the data is.
    xs = [0.0] * 40 + [4.0] * 40
    grid = [i * 0.1 for i in range(-20, 61)]
    d = gaussian_kde(xs, grid)
    area = sum(d) * 0.1
    assert 0.9 < area < 1.1, area
    assert d[grid.index(0.0)] > d[grid.index(2.0)], "a trough between the humps"

    assert is_num(1.5) and not is_num(True) and not is_num(None)
    assert show(None) == "—" and show(1.0 / 3.0) == "0.3333"
    return "ok"
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let ws = Workspace::new_empty();
    let args: [(String, ScriptValue); 0] = [];
    let out = run_script(checks, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("every assertion holds");
    let ScriptOutput::Node(node) = out else {
        panic!("returns ok")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("ok".to_owned())
    );
}

/// A scatter built from the sample draws, and answers the whole protocol.
#[test]
fn a_scatter_draws_and_answers_the_protocol() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 20, "the scatter painted its marks");

    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, RowKeys()))"
        ),
        "90",
        "every row of the sample is placeable"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "90",
        "and every one of them was drawn"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['kind']"
        ),
        "scatter"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, DrawnPoint(10 ** 9))"
        ),
        "None",
        "an id past the end is a clean None"
    );
    assert!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, PointLabel(3))"
        )
        .contains("body_mass_g"),
        "the label names what was plotted"
    );
    assert!(
        ask(
            &ws,
            root,
            "sorted(dex.snapshot.send_request(target, RowValues(3)).keys())"
        )
        .contains("species"),
        "the record behind a row is the whole record"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SourceTable()).num_rows"
        ),
        "90",
        "and the table behind the view comes back whole"
    );
}

/// Every layout paints from the sample without raising.
#[test]
fn every_layout_paints() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let views = [
        "build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')",
        "build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm', color='species')",
        "build_plot(dex.ws, Bars, x='species')",
        "build_plot(dex.ws, Strip, x='body_mass_g')",
        "build_plot(dex.ws, Strip, x='species', y='body_mass_g')",
        "build_plot(dex.ws, Violin, x='species', y='body_mass_g')",
        "build_plot(dex.ws, Violin, x='body_mass_g')",
        "build_plot(dex.ws, Heatmap, x='species', y='island')",
    ];
    for view in views {
        let (mut ws, _root) = built(&format!("def transform():\n    return {view}\n"));
        let ctx = context();
        assert!(drawn(&mut ws, &ctx) > 10, "{view} painted");
    }
}

/// The writes: pushing a selection into a view is what links two of them.
#[test]
fn a_selection_can_be_pushed_in() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    assert_eq!(
        ask(&ws, root, "dex.snapshot.send_request(target, Selection())"),
        "None",
        "nothing is selected to begin with"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SetSelection(7))"
        ),
        "7",
        "a row can be pushed in from outside"
    );
    assert_eq!(
        ask(&ws, root, "dex.snapshot.send_request(target, Selection())"),
        "7",
        "and it stuck"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SetSelection(10 ** 9))"
        ),
        "None",
        "a row that does not exist selects nothing"
    );
}

/// Re-pointing a view at other columns changes what it draws.
#[test]
fn an_encoding_can_be_pushed_in() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    assert!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SetEncoding(y='bill_length_mm'))['y']"
        ) == "bill_length_mm",
        "the channel took"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SetEncoding(y='not a column'))['y']"
        ),
        "bill_length_mm",
        "a column that does not exist is ignored rather than obeyed"
    );
    assert!(drawn(&mut ws, &ctx) > 20, "and it still draws");
}

/// A table with a lineage column and a key, for the tree and the join.
const LINEAGES: &str = r#"
def sample():
    lineages = [
        "Bacteria;Proteobacteria;Gammaproteobacteria;Escherichia",
        "Bacteria;Proteobacteria;Gammaproteobacteria;Salmonella",
        "Bacteria;Proteobacteria;Alphaproteobacteria;Rhizobium",
        "Bacteria;Firmicutes;Bacilli;Bacillus",
        "Bacteria;Firmicutes;Bacilli;Staphylococcus",
        "Archaea;Euryarchaeota;Methanobacteria;Methanobrevibacter",
    ]
    return {
        "accession": ["GCA_%03d" % i for i in range(len(lineages))],
        "lineage": lineages,
        "phylum": [l.split(";")[1] for l in lineages],
        "genes": [4200, 4500, 6100, 4100, 2700, 1800],
        "gc": [50.8, 52.2, 61.0, 43.5, 32.8, 31.0],
        "length_mb": [4.6, 4.9, 6.8, 4.2, 2.8, 1.7],
    }
"#;

/// The tree draws both ways, and places every row on the leaf its lineage ends on.
#[test]
fn a_phylogeny_draws_in_both_shapes() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    for shape in ["hierarchical", "circular"] {
        let (mut ws, root) = built(&format!(
            "{LINEAGES}\n\
             def transform():\n    \
                 return build_plot(dex.ws, Phylogeny, sample(), x='lineage',\n                 \
                                   color='phylum', shape='{shape}')\n"
        ));
        let ctx = context();
        assert!(drawn(&mut ws, &ctx) > 20, "the {shape} tree painted");
        assert_eq!(
            ask(
                &ws,
                root,
                "len(dex.snapshot.send_request(target, DrawnPoints()))"
            ),
            "6",
            "every row sits on the leaf its lineage ends on ({shape})"
        );
        assert!(
            ask(
                &ws,
                root,
                "dex.snapshot.send_request(target, PointLabel(0))"
            )
            .contains("Escherichia"),
            "and the readout names the whole lineage"
        );
    }
}

/// The tree is one node with two shapes, so both agree about the rows.
#[test]
fn both_tree_shapes_place_the_same_rows() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let script = format!(
        "{LINEAGES}\n\
         def transform():\n    \
             frame = Frame(sample())\n    \
             a = build_plot(dex.ws, Phylogeny, frame=frame, x='lineage', shape='hierarchical')\n    \
             b = build_plot(dex.ws, Phylogeny, frame=frame, x='lineage', shape='circular')\n    \
             same = sorted(a.tree.row_leaf.items()) == sorted(b.tree.row_leaf.items())\n    \
             return \"same\" if same else \"different\"\n"
    );
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let ws = Workspace::new_empty();
    let args: [(String, ScriptValue); 0] = [];
    let out = run_script(
        &script,
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    )
    .expect("both trees build");
    let ScriptOutput::Node(node) = out else {
        panic!("returns a verdict")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("same".to_owned()),
        "one tree, drawn two ways"
    );
}

/// A circos draws its ring, its tracks and its chords.
#[test]
fn a_circos_draws_with_tracks_and_links() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(&format!(
        "{LINEAGES}\n\
         def transform():\n    \
             return build_plot(dex.ws, Circos, sample(), x='phylum',\n                 \
                               tracks=['genes', 'gc'], link='accession',\n                 \
                               key='accession')\n"
    ));
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 30, "the circos painted");
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "6",
        "every row sits somewhere on the ring"
    );
}

/// The 3D view projects, and dragging turns it.
#[test]
fn a_3d_scatter_projects_and_turns() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter3D, x='body_mass_g', y='flipper_mm',\n             \
                               z='bill_length_mm', color='species')\n",
    );
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 20, "the cloud painted");
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "90",
    );
    let before = ask(
        &ws,
        root,
        "round(dex.snapshot.send_request(target, DrawnPoint(0))[0], 2)",
    );

    // A drag across the middle of the view.
    let (from, to) = (egui::pos2(360.0, 260.0), egui::pos2(430.0, 260.0));
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(from)]);
    frame(
        &mut ws,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(to)]);
    frame(
        &mut ws,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    frame(&mut ws, &ctx, vec![]);
    let after = ask(
        &ws,
        root,
        "round(dex.snapshot.send_request(target, DrawnPoint(0))[0], 2)",
    );
    assert_ne!(before, after, "the drag turned the cloud");
}

/// Two views, joined: lines between the same record in each, and their two
/// tables put together — both written against the protocol, neither knowing
/// what kind of view it is talking to.
#[test]
fn two_views_can_be_linked_and_joined() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let link = r#"
def transform():
    ws = dex.ws
    frame = Frame(sample())
    tree = ws.insert_node_dyn(build_plot(ws, Phylogeny, frame=frame, x="lineage"))
    scatter = ws.insert_node_dyn(build_plot(ws, Scatter, frame=frame, x="genes", y="gc"))
    joiner = link_views(ws, tree, scatter)
    return dex.VerticalLayout.build(ws, [tree, scatter, joiner], 0.0)
"#;
    let (mut ws, root) = built(&format!("{LINEAGES}{link}"));
    let ctx = context();
    assert!(
        drawn(&mut ws, &ctx) > 30,
        "both views and their links painted"
    );

    // The join, run against the workspace once both views are live in it: a
    // snapshot only holds what was there when it was taken.
    let join = r#"
def transform():
    (a, b, _joiner) = dex.snapshot.owned_refs(root)
    pairs = row_correspondence(dex.snapshot, a, b)
    table = joined_table(dex.snapshot, a, b)
    return "%d %d %d" % (len(pairs), table.num_rows, table.num_columns)
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("root".to_owned(), ScriptValue::Node(root))];
    let out = run_script(join, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the join runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns counts")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("6 6 12".to_owned()),
        "six records in common, and both tables' columns side by side"
    );
}

/// The explorer puts its view on a plane, and passes the protocol down to it —
/// so anything that works on a bare layout works on the explorer.
#[test]
fn the_explorer_drives_a_view_on_a_plane() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built("def transform():\n    return build_explorer(dex.ws)\n");
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 20, "the explorer painted a view");

    // Three dropdowns and the plane. The view is not owned here: the plane owns
    // it, along with the item holding it and the readout in front of it.
    use dex_core::refs::NodeRefs;
    let mut owned = Vec::new();
    ws.get_node(root)
        .unwrap()
        .owned_refs(&mut |uid| owned.push(uid));
    assert_eq!(owned.len(), 4, "a mode dropdown, x, y, and the plane");

    let canvas = owned[3];
    let body = ws
        .send_request(canvas, dex_nodes::layouts::canvas::layout::CanvasChildren)
        .expect("the plane is a canvas");
    assert_eq!(body.len(), 1, "the view is the one thing on it");
    let front = ws
        .send_request(
            canvas,
            dex_nodes::layouts::canvas::layout::CanvasLayerNodes {
                layer: dex_nodes::layouts::canvas::layout::Layer::Foreground,
            },
        )
        .unwrap_or_default();
    assert_eq!(
        front.len(),
        3,
        "its title, its figures and its readout in front, all at their own size \
         whatever the plane is zoomed to"
    );
    let behind = ws
        .send_request(
            canvas,
            dex_nodes::layouts::canvas::layout::CanvasLayerNodes {
                layer: dex_nodes::layouts::canvas::layout::Layer::Background,
            },
        )
        .unwrap_or_default();
    assert_eq!(behind.len(), 1, "and the grid behind it");

    // And the protocol reaches the view behind all of that.
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "90",
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SourceTable()).num_rows"
        ),
        "90",
    );
}

/// The layout follows the data, not a list: a mode and two columns is all
/// anybody says, and which picture that comes to is a question about types.
#[test]
fn the_explorer_picks_its_layout_from_the_columns() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built("def transform():\n    return build_explorer(dex.ws)\n");
    let ctx = context();
    drawn(&mut ws, &ctx);

    use dex_core::refs::NodeRefs;
    let mut owned = Vec::new();
    ws.get_node(root)
        .unwrap()
        .owned_refs(&mut |uid| owned.push(uid));
    let (mode_dd, x_dd, y_dd) = (owned[0], owned[1], owned[2]);

    // The sample's columns, in order: species, island, sex, body_mass_g,
    // flipper_mm, bill_length_mm.
    let choose = |ws: &mut Workspace, dd: NodeUid, index: usize| {
        ws.submit_action_dyn(Action {
            dest: dd,
            description: "choose".into(),
            body: Box::new(dex_nodes::primitives::dropdown::SetDropdownSelection { index }),
        });
        ws.process_pending();
    };
    let kind = |ws: &Workspace| {
        ask(
            ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['kind']",
        )
    };

    // Univariate over a categorical column: how often each category occurs.
    assert_eq!(kind(&ws), "bars", "it opens univariate on the first column");

    // Univariate over a continuous one: every row at its value, with the shape
    // of the column drawn around them.
    choose(&mut ws, x_dd, 3);
    drawn(&mut ws, &ctx);
    assert_eq!(kind(&ws), "violin");

    // Bivariate follows the pair — the whole matrix, not just the numeric
    // corner of it.
    choose(&mut ws, mode_dd, 1);
    choose(&mut ws, y_dd, 4);
    drawn(&mut ws, &ctx);
    assert_eq!(kind(&ws), "scatter", "continuous x continuous");

    choose(&mut ws, x_dd, 0);
    drawn(&mut ws, &ctx);
    assert_eq!(kind(&ws), "violin", "categorical x continuous");

    // And the other way round: whichever way they were chosen, the category is
    // what the measure is split by.
    choose(&mut ws, x_dd, 3);
    choose(&mut ws, y_dd, 0);
    drawn(&mut ws, &ctx);
    assert_eq!(kind(&ws), "violin", "continuous x categorical");
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['x']"
        ),
        "species",
        "with the category on x, however it was picked"
    );

    choose(&mut ws, y_dd, 1);
    choose(&mut ws, x_dd, 0);
    drawn(&mut ws, &ctx);
    assert_eq!(kind(&ws), "heatmap", "categorical x categorical");

    // Through all of that it stayed the same node, on the same plane, over the
    // same table: the view becomes another layout rather than being rebuilt.
    let mut after = Vec::new();
    ws.get_node(root)
        .unwrap()
        .owned_refs(&mut |uid| after.push(uid));
    assert_eq!(after, owned, "nothing was rebuilt");
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SourceTable()).num_rows"
        ),
        "90",
        "over the same table, read once"
    );
    assert!(drawn(&mut ws, &ctx) > 20, "and it is still drawing");
}

/// The examples that are now thin wrappers all still build and paint.
#[test]
fn the_rewritten_examples_build_and_paint() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    for (name, source) in [
        ("circos.py", include_str!("../../../examples/circos.py")),
        ("circos2.py", include_str!("../../../examples/circos2.py")),
        ("circos3.py", include_str!("../../../examples/circos3.py")),
        (
            "circos_table.py",
            include_str!("../../../examples/circos_table.py"),
        ),
    ] {
        let (mut ws, _root) = built(source);
        let ctx = context();
        assert!(
            drawn(&mut ws, &ctx) > 20,
            "{name} built and painted from its own sample"
        );
    }
}

/// Clicking a mark opens the record as a real `Table` node — the whole row,
/// with its own column types, not a drawn imitation of one.
///
/// It has to be exercised through a *draw*. A view builds the row's table by
/// inserting a node, and an insert is queued on the workspace handle of
/// whatever asked; a script querying a snapshot brings its own throwaway
/// handle, so the uid it gets back names a node that never arrives. During a
/// draw the handle is the live workspace's, which is the only path that
/// actually seats anything.
#[test]
fn a_selected_row_becomes_a_real_table_node() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    fn tables(ws: &Workspace) -> Vec<NodeUid> {
        ws.live_ids()
            .into_iter()
            .filter(|uid| {
                ws.get_node(*uid).is_some_and(|node| {
                    (*node)
                        .as_any_ref()
                        .is::<dex_nodes::primitives::table::Table>()
                })
            })
            .collect()
    }
    assert!(
        tables(&ws).is_empty(),
        "nothing selected, so no row has been sliced out"
    );

    // Select a row and draw: the overlay asks the view for that row's table.
    let _ = ask(
        &ws,
        root,
        "dex.snapshot.send_request(target, SetSelection(3))",
    );
    drawn(&mut ws, &ctx);
    ws.process_pending();
    drawn(&mut ws, &ctx);

    let seated = tables(&ws);
    assert_eq!(seated.len(), 1, "one table, for the selected row");
    let node = ws.get_node(seated[0]).unwrap();
    let table = (*node)
        .as_any_ref()
        .downcast_ref::<dex_nodes::primitives::table::Table>()
        .unwrap();
    assert_eq!(table.batch().num_rows(), 1, "one row: the selected record");
    assert_eq!(
        table.batch().num_columns(),
        6,
        "and the whole record, not just the two plotted columns"
    );
    // Real Arrow types, which is the whole reason for a Table over a drawn grid.
    let schema = table.batch().schema();
    let species = schema.field_with_name("species").expect("the text column");
    assert!(
        matches!(species.data_type(), arrow::datatypes::DataType::Utf8),
        "text stayed text: {:?}",
        species.data_type()
    );

    // Drawing again reuses it rather than piling up a node per frame.
    drawn(&mut ws, &ctx);
    ws.process_pending();
    assert_eq!(tables(&ws).len(), 1, "one table, not one per frame");

    // Selecting another row replaces it: the old one does not leak.
    let _ = ask(
        &ws,
        root,
        "dex.snapshot.send_request(target, SetSelection(4))",
    );
    drawn(&mut ws, &ctx);
    ws.process_pending();
    drawn(&mut ws, &ctx);
    ws.process_pending();
    assert_eq!(
        tables(&ws).len(),
        1,
        "the previous row's table went when the selection moved on"
    );
}

/// What the sidebar offers is a pair of lambdas you wire things into — named
/// for what they do, declaring what they want, and holding a script you can
/// edit. This pins the first: a view of a table.
#[test]
fn the_offer_is_a_lambda_that_takes_a_table() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let (offers, error) = dex_nodes::prelude_prototypes::read(PRELUDE, &handle);
    assert_eq!(error, None, "the prelude scans clean");
    assert_eq!(
        offers
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        ["Data explorer"],
        "the one offer, named for what it does — connecting two views is a \
         worked example (examples/connect_views.py), not a sidebar offer"
    );

    // Seat it the way the sidebar does, with everything its factory queued.
    let (_, template, _) = offers[0].clone();
    let uid = ws.action_handle().insert_node_dyn(template);
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();

    let node = ws.get_node(uid).expect("the offer is live");
    assert!(
        (*node)
            .as_any_ref()
            .is::<dex_nodes::composites::lambda::Lambda>(),
        "it is a lambda, not the explorer itself: a view of a table has to be \
         given the table, and being wired into is how that happens"
    );

    // One argument, called `thisData`, declared to be a table.
    let row = ws
        .send_request(
            uid.cast::<dex_nodes::composites::lambda::Lambda>(),
            dex_nodes::composites::lambda::LambdaArgsNode,
        )
        .expect("the lambda has an argument row");
    let args = ws
        .send_request(
            row.cast::<dex_nodes::composites::lambda::LambdaArgs>(),
            dex_nodes::composites::lambda::ArgDeclarations,
        )
        .expect("the row reports its arguments");
    assert_eq!(args.len(), 1, "one argument");
    assert_eq!(args[0].0, "thisData", "named for what the script reads");
    assert_eq!(
        args[0].2,
        dex_nodes::argtypes::ArgType::Table,
        "declared a table, so the port says what belongs in it"
    );

    // The row reads `label: name (kind)`, so the whole line says
    // "for: thisData (a table)" — what wiring something in is *for*, which a
    // parameter name on its own does not say.
    let arg_nodes = ws
        .send_request(
            row.cast::<dex_nodes::composites::lambda::LambdaArgs>(),
            dex_nodes::composites::lambda::ArgNodes,
        )
        .expect("the row lists its arguments");
    assert_eq!(
        ws.send_request(
            arg_nodes[0].cast::<dex_nodes::composites::lambda::LambdaArg>(),
            dex_nodes::composites::lambda::ArgLabel,
        )
        .as_deref(),
        Some("for"),
    );

    // And it holds a script that runs: wire the sample in and it builds.
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args: [(String, ScriptValue); 0] = [];
    let source = "def transform():\n    return build_explorer(dex.ws, sample_table())\n";
    let out = run_script(source, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the script the lambda carries runs");
    let ScriptOutput::Node(explorer) = out else {
        panic!("it builds an explorer")
    };
    let seated = ws.action_handle().insert_node_dyn(explorer);
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(seated);
    ws.process_pending();
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 20, "and what it builds draws");
}

/// The picture is on a plane, so it is panned and zoomed rather than squeezed
/// into whatever box the explorer happens to be drawn at.
///
/// Checked through `DrawnPoints`, which answers in screen coordinates: if the
/// plane really is moving under the view, the marks land somewhere else. That
/// is also the thing that would silently break — a view drawn straight into the
/// box still paints, it just stops being navigable.
#[test]
fn the_explorer_s_picture_pans_and_zooms() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    // A scatter, because that is one of the two kinds worth a plane: a mark per
    // row, always more of it than fits. A bar chart is drawn straight into its
    // box, and has nothing to pan.
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_explorer(dex.ws, mode='Bivariate',\n                 \
                                   x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    let mark = |ws: &Workspace| {
        ask(
            ws,
            root,
            "'%.1f,%.1f' % dex.snapshot.send_request(target, DrawnPoint(0))",
        )
    };
    let before = mark(&ws);
    drawn(&mut ws, &ctx);
    assert_eq!(
        mark(&ws),
        before,
        "an idle frame leaves the marks where they were, so what follows is the \
         plane moving and not the picture settling"
    );

    // Over the plane, well clear of the control bar along the top.
    let over = egui::pos2(400.0, 320.0);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(over)]);
    frame(&mut ws, &ctx, vec![egui::Event::Zoom(2.0)]);
    drawn(&mut ws, &ctx);
    let zoomed = mark(&ws);
    assert_ne!(before, zoomed, "magnifying the plane moved the marks");

    // Dragging empty background pans it: the view is a *static* item, which
    // declines an inspector, so the drag reaches the plane rather than being
    // taken as a gesture on the thing sitting on it.
    let to = egui::pos2(330.0, 260.0);
    frame(
        &mut ws,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: over,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(to)]);
    frame(
        &mut ws,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    drawn(&mut ws, &ctx);
    assert_ne!(zoomed, mark(&ws), "dragging the plane moved them again");
}

/// A lineage splits into its ranks, rank prefixes come off, and it stops where
/// the data stops saying anything.
#[test]
fn a_lineage_splits_into_ranked_names() {
    dex_nodes::scripting::init_python();
    let _ = has_pyarrow();
    let checks = r#"
def transform():
    assert split_lineage("A;B;C") == ["A", "B", "C"]
    # Rank prefixes come off, so the same lineage makes the same tree whether
    # or not whoever wrote it labelled the ranks.
    assert split_lineage("d__Bacteria;p__Bacillota") == ["Bacteria", "Bacillota"]
    # An unplaced rank ends the lineage: everything unclassified is not one clade.
    assert split_lineage("A;B;unclassified;D") == ["A", "B"]
    assert split_lineage("A;;C") == ["A"]
    assert split_lineage("  A ; B ") == ["A", "B"]
    assert split_lineage("") == []
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");
}

/// Lineages sharing a prefix share nodes — and names that merely look alike do
/// not. A genus is identified by its whole path, because homonyms across the
/// tree of life are the rule rather than the exception. Parents sit on the
/// midpoint of their children, which is what makes a dendrogram readable.
#[test]
fn the_tree_shares_prefixes_and_centres_parents() {
    dex_nodes::scripting::init_python();
    let _ = has_pyarrow();
    let checks = r#"
def transform():
    tree = Tree(list(enumerate([
        "A;B;X",
        "A;B;Y",
        "A;C;X",
    ])))
    # One root, and "A;B" is one node reached by two rows.
    assert tree.roots == [("A",)], tree.roots
    assert tree.nodes[("A", "B")]["weight"] == 2
    # The two X's are different nodes: same name, different lineage.
    assert ("A", "B", "X") in tree.nodes and ("A", "C", "X") in tree.nodes
    assert tree.nodes[("A", "B", "X")]["rows"] == [0]
    assert tree.nodes[("A", "C", "X")]["rows"] == [2]

    # Three leaves in slots 0, 1, 2. A parent is centred on its *immediate*
    # children, not on all the leaves under it: "A;B" sits between its two at
    # 0.5, and the root between "A;B" (0.5) and "A;C" (2.0) at 1.25. Which is
    # what makes an unbalanced tree lean towards its heavier side.
    assert len(tree.leaves) == 3
    assert tree.slot[("A", "B")] == 0.5
    assert tree.slot[("A", "C")] == 2.0
    assert tree.slot[("A",)] == 1.25

    # Every row ends on a leaf, which is what makes DrawnPoints answerable.
    assert sorted(tree.row_leaf) == [0, 1, 2]
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");
}

/// The sideways shape is the third one, and it places the same rows as the
/// other two.
#[test]
fn a_tree_draws_sideways_too() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(&format!(
        "{LINEAGES}\n\
         def transform():\n    \
             return build_plot(dex.ws, Phylogeny, sample(), x='lineage', shape='sideways')\n"
    ));
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 20, "the sideways tree painted");
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "6",
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['kind']"
        ),
        "phylogeny",
        "still the same layout, just laid out the other way"
    );
}

/// The other way a table carries a tree: one row per node, pointing at its
/// parent. Branch lengths and tip order come from the columns that hold them.
#[test]
fn a_tree_can_be_read_from_an_edge_list() {
    dex_nodes::scripting::init_python();
    let _ = has_pyarrow();
    let checks = r#"
def transform():
    #      r
    #     / \
    #    a   b
    #   / \
    #  c   d
    tree = Tree.from_edges(
        ids=["r", "a", "b", "c", "d"],
        parents=[None, "r", "r", "a", "a"],
        depths=[0.0, 0.4, 0.9, 1.1, 1.3],
        leaf_order=[None, None, 2, 0, 1],
    )
    assert tree.roots == [("r",)], tree.roots
    assert sorted(tree.nodes[("r",)]["children"]) == [("a",), ("b",)]
    assert sorted(tree.nodes[("a",)]["children"]) == [("c",), ("d",)]

    # Depth is the distance column, so branch lengths are to scale.
    assert tree.nodes[("d",)]["depth"] == 1.3
    assert tree.max_depth == 1.3

    # Weight counts the subtree, so a dot can be sized by it.
    assert tree.nodes[("a",)]["weight"] == 3
    assert tree.nodes[("r",)]["weight"] == 5

    # The tool's own tip order wins: c, d, b — not the order they were walked.
    assert tree.leaves == [("c",), ("d",), ("b",)], tree.leaves

    # Every row is a node, so every row is placeable — internal ones included.
    assert sorted(tree.row_leaf) == [0, 1, 2, 3, 4]

    # Without a distance column, depth counts hops instead.
    hops = Tree.from_edges(["r", "a", "c"], [None, "r", "a"])
    assert hops.nodes[("c",)]["depth"] == 2
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");
}

/// A `Phylogeny` over an edge-list table draws and places every node.
#[test]
fn a_phylogeny_draws_from_an_edge_list() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let edges = r#"
def edges():
    return {
        "node": ["r", "a", "b", "c", "d", "e"],
        "parent": [None, "r", "r", "a", "a", "b"],
        "depth": [0.0, 0.4, 0.5, 1.1, 1.3, 1.0],
        "leaf_order": [None, None, None, 0, 1, 2],
        "clade": ["root", "left", "right", "left", "left", "right"],
    }
"#;
    let (mut ws, root) = built(&format!(
        "{edges}\n\
         def transform():\n    \
             return build_plot(dex.ws, Phylogeny, edges(), x='node', parent='parent',\n                 \
                               depth='depth', leaf_order='leaf_order', color='clade',\n                 \
                               shape='circular')\n"
    ));
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 15, "the edge-list tree painted");
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "6",
        "every node is a row, and every row was placed"
    );
    assert!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, PointLabel(3))"
        )
        .contains('c'),
        "a node reads as its own name, not as a path"
    );
}

/// A branch length that does not grow from a parent to its child still lays out.
///
/// The regression: slots were worked out in order of the depth *column*, on the
/// assumption that a child is always deeper than its parent. With hop-counted
/// depth that holds. With branch lengths it does not — a zero-length branch
/// gives parent and child the same distance, and a rounded column can give the
/// child less — so a parent was reached first and asked for a slot that had not
/// been worked out yet, which came out as `KeyError: (<node id>,)`.
#[test]
fn a_tree_lays_out_whatever_the_depth_column_says() {
    dex_nodes::scripting::init_python();
    let _ = has_pyarrow();
    let checks = r#"
def transform():
    # b sits at the same distance as its parent a (a zero-length branch), and
    # c at *less* than its parent b.
    tree = Tree.from_edges(
        ids=["r", "a", "b", "c"],
        parents=[None, "r", "a", "b"],
        depths=[0.0, 0.7, 0.7, 0.6],
    )
    assert sorted(tree.slot) == sorted(tree.nodes), "every node was placed"
    # One chain, so every node sits over the single leaf at slot 0.
    assert tree.slot[("r",)] == 0.0, tree.slot

    # A parent column that points in a circle leaves nodes no root can reach.
    # They are placed after everything else rather than left out, so the
    # picture still draws and shows them.
    cyclic = Tree.from_edges(ids=["x", "y"], parents=["y", "x"])
    assert sorted(cyclic.slot) == sorted(cyclic.nodes), cyclic.slot
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");
}

/// And the layout draws it, which is where the failure actually showed up.
#[test]
fn a_phylogeny_draws_a_tree_with_flat_branches() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let edges = r#"
def edges():
    # `b` is an internal node sitting at the same distance as its parent `a` —
    # a zero-length branch, which is what a real tree looks like wherever the
    # tool could not resolve the order. The tips hang below it, so working the
    # slots out by depth reaches `a` before `b` has one.
    return {
        "node": ["r", "a", "b", "b1", "b2", "c", "c1"],
        "parent": [None, "r", "a", "b", "b", "r", "c"],
        "depth": [0.0, 0.5, 0.5, 0.9, 1.0, 0.4, 0.8],
        "leaf_order": [None, None, None, 0, 1, None, 2],
    }
"#;
    let (mut ws, root) = built(&format!(
        "{edges}\n\
         def transform():\n    \
             return build_plot(dex.ws, Phylogeny, edges(), x='node', parent='parent',\n                 \
                               depth='depth', leaf_order='leaf_order', shape='circular')\n"
    ));
    let ctx = context();
    assert!(drawn(&mut ws, &ctx) > 10, "the tree painted");
    assert_eq!(
        ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, DrawnPoints()))"
        ),
        "7",
        "every node placed, flat branches and all"
    );
}

/// The axes are worked out for what is on screen, not baked into the picture.
///
/// Zooming into a crowded corner should *subdivide* the axis — more ticks, at
/// finer values — rather than stretching the ones the whole of the data called
/// for. That is only possible because the grid is a background that asks the
/// view how it maps data onto the plane, instead of marks the view painted once
/// at one size.
#[test]
fn the_axes_are_drawn_for_the_visible_range() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return plot_on_plane(dex.ws, build_plot(\n        \
                 dex.ws, Scatter, x='body_mass_g', y='flipper_mm'))\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    // The view publishes its mapping; the background is written against it.
    let plot = {
        use dex_core::refs::NodeRefs;
        let mut owned = Vec::new();
        ws.get_node(root)
            .unwrap()
            .owned_refs(&mut |uid| owned.push(uid));
        ws.send_request(root, dex_nodes::layouts::canvas::layout::CanvasChildren)
            .and_then(|items| items.first().copied())
            .and_then(|item| {
                ws.send_request(item, dex_nodes::layouts::canvas::nodes::CanvasNodeChild)
            })
            .expect("the view is the item on the plane")
    };
    assert!(
        ask(
            &ws,
            plot,
            "dex.snapshot.send_request(target, PlotScale())['y']['kind']"
        ) == "value",
        "the scatter publishes a continuous y axis"
    );

    // How many tick captions the grid drew, by counting the text it painted.
    let captions = |ws: &mut Workspace, ctx: &egui::Context| -> Vec<f64> {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            },
        );
        fn walk(shape: &egui::Shape, seen: &mut Vec<f64>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, seen)),
                egui::Shape::Text(t) => {
                    // A tick caption is a bare number.
                    if let Ok(v) = t.galley.text().trim().parse::<f64>() {
                        seen.push(v);
                    }
                }
                _ => {}
            }
        }
        let mut seen = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut seen));
        seen
    };
    let before = captions(&mut ws, &ctx);
    assert!(
        before.len() > 1,
        "the grid captioned its ticks ({before:?})"
    );
    let gap = |vs: &[f64]| -> f64 {
        let mut sorted = vs.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|d| *d > 0.0)
            .fold(f64::INFINITY, f64::min)
    };
    let coarse = gap(&before);

    // Magnify hard: the same window now covers a much smaller slice of the
    // data, so the axis has to find finer values to put ticks at.
    let over = egui::pos2(400.0, 300.0);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(over)]);
    for _ in 0..3 {
        frame(&mut ws, &ctx, vec![egui::Event::Zoom(2.0)]);
    }
    drawn(&mut ws, &ctx);
    let after = captions(&mut ws, &ctx);
    assert!(
        after.len() > 1,
        "there are still captions after zooming in ({after:?})"
    );

    // The step itself got finer. Ticks baked into the picture would come back
    // at the same values however far in the plane was zoomed — spread further
    // apart, and most of them off screen. These were worked out again for the
    // window that is actually showing.
    let fine = gap(&after);
    assert!(
        fine < coarse,
        "the axis subdivided as it was magnified \
         (step was {coarse}, now {fine}; before {before:?}, after {after:?})"
    );
}

/// A view keeps what was expensive to get, and keeps it *itself*.
///
/// This is what the drilling in `superphylogeny.py` rests on: a genome fetched
/// for one tip is kept by the tree rather than by the readout that asked for
/// it, because a readout is chrome and gets rebuilt, and a network round trip
/// should not go with it. Being the view's means a clone copies it and a delete
/// takes it away.
#[test]
fn a_view_keeps_what_was_expensive_to_get() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    assert_eq!(
        ask(&ws, root, "dex.snapshot.send_request(target, KeptNode(7))"),
        "None",
        "nothing kept for a key nobody has asked about"
    );

    // Hand it something to keep, then ask for it back.
    let script = r#"
def transform():
    made = dex.ws.insert_node_dyn("an expensive thing")
    kept = dex.snapshot.send_request(target, KeepNode(7, made))
    again = dex.snapshot.send_request(target, KeptNode(7))
    # The first one wins, so a second asker shares it rather than replacing it.
    other = dex.ws.insert_node_dyn("a second thing")
    same = dex.snapshot.send_request(target, KeepNode(7, other))
    return "%s %s %s" % (kept == made, again == made, same == made)
"#;
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(root))];
    let out = run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the script runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns three verdicts")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("True True True".to_owned()),
        "it kept what it was given, hands it back, and the first one wins"
    );
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();

    // And it is the view's own, so a clone copies it and a delete takes it away
    // rather than leaving it behind with nothing pointing at it.
    let check = r#"
def transform():
    kept = dex.snapshot.send_request(target, KeptNode(7))
    owned = dex.snapshot.owned_refs(target)
    assert kept is not None, "it is still keeping it"
    assert kept in owned, "and it is among what the view owns"
    return "ok"
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let out = run_script(check, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the ownership check runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns ok")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("ok".to_owned())
    );
}

/// A plane is drawn only for the layouts that earn one.
///
/// A scatter or a violin has a mark per row: there is always more of it than
/// fits. A bar chart has a mark per category — it is exactly as big as it needs
/// to be, and a plane would add a gesture that does nothing.
#[test]
fn only_the_layouts_worth_panning_get_a_plane() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let checks = r#"
def transform():
    wants = {name: layout.WANTS_PLANE for (name, layout) in LAYOUTS_BY_KIND.items()}
    assert wants["scatter"] and wants["violin"], wants
    assert not wants["bars"] and not wants["heatmap"], wants
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");

    // And the explorer follows it: a bar chart draws straight into its box, so
    // zooming over it does nothing at all.
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_explorer(dex.ws, mode='Univariate', x='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['kind']"
        ),
        "bars",
    );
    let mark = |ws: &Workspace| {
        ask(
            ws,
            root,
            "'%.1f,%.1f' % dex.snapshot.send_request(target, DrawnPoint(0))",
        )
    };
    let before = mark(&ws);
    let over = egui::pos2(400.0, 320.0);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(over)]);
    for _ in 0..3 {
        frame(&mut ws, &ctx, vec![egui::Event::Zoom(2.0)]);
    }
    drawn(&mut ws, &ctx);
    assert_eq!(
        mark(&ws),
        before,
        "there is no plane under a bar chart, so there is nothing to zoom"
    );

    // It also draws its own axes, because there is no plane to draw them.
    assert_eq!(
        ask(
            &ws,
            root,
            "str(dex.snapshot.send_request(target, SetChrome(True)))"
        ),
        "True",
        "a view drawn straight into a box draws its own decoration"
    );
}

/// Category captions are all drawn or none: never a row of truncated stumps.
#[test]
fn crowded_category_captions_are_dropped_rather_than_truncated() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    // Painted text that is one of the sample's species names.
    let named = |ws: &mut Workspace, ctx: &egui::Context| -> usize {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, seen: &mut usize) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, seen)),
                egui::Shape::Text(t)
                    if ["Adelie", "Gentoo", "Chinstrap"].contains(&t.galley.text().trim()) =>
                {
                    *seen += 1;
                }
                _ => {}
            }
        }
        let mut seen = 0;
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut seen));
        seen
    };

    // Three categories across a wide plot: room for all of them.
    let (mut ws, _root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Bars, x='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    assert!(named(&mut ws, &ctx) >= 3, "three names, and room for them");

    // Now the case that made this necessary: many categories with long names,
    // so the slots are a few pixels each. None of them fits, so none is drawn —
    // a stump that looks like a word is worse than no word, and the bar itself
    // still names its rows when hovered.
    let crowded = r#"
def many():
    names = ["Pseudomonadota subspecies %02d" % i for i in range(30)]
    return {"taxon": [names[i % len(names)] for i in range(240)]}


def transform():
    return build_plot(dex.ws, Bars, many(), x="taxon")
"#;
    let (mut ws, _root) = built(crowded);
    drawn(&mut ws, &ctx);
    let long_names = |ws: &mut Workspace, ctx: &egui::Context| -> usize {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, seen: &mut usize) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, seen)),
                egui::Shape::Text(t) if t.galley.text().starts_with("Pseudomonadota") => {
                    *seen += 1;
                }
                _ => {}
            }
        }
        let mut seen = 0;
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut seen));
        seen
    };
    assert_eq!(
        long_names(&mut ws, &ctx),
        0,
        "thirty long names in one axis: none drawn rather than thirty stumps"
    );
}

/// Univariate means one column, whatever the other dropdown happens to show.
///
/// The regression: the violin swapped x and y so a category would end up on x,
/// and in univariate mode that pulled in whatever the second dropdown was left
/// on — turning one distribution into a page of them, split by a column nobody
/// had chosen for the purpose.
#[test]
fn a_univariate_view_uses_only_the_one_column() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        // A y column is deliberately set, and univariate must ignore it.
        "def transform():\n    \
             return build_explorer(dex.ws, mode='Univariate',\n                 \
                                   x='body_mass_g', y='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);

    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['kind']"
        ),
        "violin",
        "a continuous column on its own is a violin of its values"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['x']"
        ),
        "body_mass_g",
        "the column that was chosen"
    );
    assert_eq!(
        ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, Encoding())['y']"
        ),
        "None",
        "and nothing on the second channel, so it is one distribution"
    );

    // One band, not one per species: the readout names only the column plotted.
    let label = ask(
        &ws,
        root,
        "dex.snapshot.send_request(target, PointLabel(0))",
    );
    assert!(
        label.contains("body_mass_g") && !label.contains("species"),
        "the mark is described by the one column it was placed by: {label}"
    );
}

/// The record in the click overlay is a real table, seated without a second
/// frame around it.
#[test]
fn the_row_table_is_seated_without_its_own_border() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Scatter, x='body_mass_g', y='flipper_mm')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    let _ = ask(
        &ws,
        root,
        "dex.snapshot.send_request(target, SetSelection(3))",
    );
    drawn(&mut ws, &ctx);
    ws.process_pending();
    drawn(&mut ws, &ctx);

    let table = ws
        .live_ids()
        .into_iter()
        .find_map(|uid| {
            ws.get_node(uid).and_then(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<dex_nodes::primitives::table::Table>()
                    .map(|t| t.bordered)
            })
        })
        .expect("the selected row became a table");
    assert!(
        !table,
        "no frame of its own: the card it sits in is already one, and two \
         borders a pixel apart read as a mistake"
    );
}

/// Every view is titled, whether or not it is on a plane.
///
/// The furniture around a picture should be the same furniture: what differs
/// between a bar chart and a scatter is the picture, not whether it says what
/// it is. A planed view gets its title from `PlotTitle` in the foreground; one
/// drawn straight into a box draws the same words itself.
#[test]
fn every_view_says_what_it_is() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let titled = |ws: &mut Workspace, ctx: &egui::Context, want: &str| -> bool {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, want: &str, seen: &mut bool) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, want, seen)),
                egui::Shape::Text(t) if t.galley.text().contains(want) => *seen = true,
                _ => {}
            }
        }
        let mut seen = false;
        out.shapes
            .iter()
            .for_each(|c| walk(&c.shape, want, &mut seen));
        seen
    };

    // A bar chart is drawn straight into its box: no plane, so it titles itself.
    let (mut ws, _root) = built(
        "def transform():\n    \
             return build_explorer(dex.ws, mode='Univariate', x='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    assert!(
        titled(&mut ws, &ctx, "Bars of species"),
        "the bar chart says what it is"
    );

    // A scatter is on a plane, and says the same kind of thing from in front.
    let (mut ws, _root) = built(
        "def transform():\n    \
             return build_explorer(dex.ws, mode='Bivariate',\n                 \
                                   x='body_mass_g', y='flipper_mm')\n",
    );
    drawn(&mut ws, &ctx);
    assert!(
        titled(&mut ws, &ctx, "Scatter of body_mass_g"),
        "and so does the scatter"
    );
}

/// The same sweep, but *drawn* — which is where the failures actually are.
///
/// A Python error mid-draw is painted as an `ErrorLayout` rather than raised,
/// so a layout that divides by zero on an empty column looks like a plot that
/// simply has nothing in it. Nothing short of drawing every combination and
/// looking for that node finds it.
#[test]
fn every_layout_draws_an_awkward_table_without_erroring() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let tables = [
        ("empty", "{'a': [], 'b': []}"),
        ("one row", "{'a': [1.0], 'b': ['x']}"),
        ("one category", "{'a': [1.0, 2.0, 3.0], 'b': ['only'] * 3}"),
        ("all null", "{'a': [None] * 3, 'b': [None] * 3}"),
        ("constant", "{'a': [5.0] * 3, 'b': ['p', 'q', 'r']}"),
        ("single column", "{'a': [1.0, 2.0, 3.0, 4.0]}"),
        ("one distinct", "{'a': [2.0, 2.0], 'b': ['p', 'p']}"),
    ];
    let layouts = [
        "Scatter",
        "Bars",
        "Strip",
        "Violin",
        "Heatmap",
        "Scatter3D",
        "Phylogeny",
        "Circos",
    ];

    // A draw error is *painted*, not registered — `draw_error` hands the
    // message straight to `ctx.draw_node`, so nothing lands in the workspace to
    // look for afterwards. The text on the screen is the only evidence.
    let painted_error = |ws: &mut Workspace, ctx: &egui::Context| -> Option<String> {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, found: &mut Option<String>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, found)),
                egui::Shape::Text(t)
                    if t.galley.text().contains("draw error")
                        || t.galley.text().contains("Traceback") =>
                {
                    *found = Some(t.galley.text().to_owned());
                }
                _ => {}
            }
        }
        let mut found = None;
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut found));
        found
    };

    let mut broken: Vec<String> = Vec::new();
    for (name, columns) in tables {
        for layout in layouts {
            let (mut ws, _root) = built(&format!(
                "def transform():\n    return build_plot(dex.ws, {layout}, {columns})\n"
            ));
            let ctx = context();
            drawn(&mut ws, &ctx);
            if let Some(message) = painted_error(&mut ws, &ctx) {
                broken.push(format!("{name} / {layout}: {message}"));
                continue;
            }
            // And with a row selected, which is what opens the overlay and
            // slices the record out into a table of its own.
            let _ = ask(
                &ws,
                _root,
                "dex.snapshot.send_request(target, SetSelection(0))",
            );
            drawn(&mut ws, &ctx);
            ws.process_pending();
            if let Some(message) = painted_error(&mut ws, &ctx) {
                broken.push(format!("{name} / {layout}, selected: {message}"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "layouts that error while drawing:\n{}",
        broken.join("\n")
    );
}

/// The explorer, walked through every mode and column pair on an awkward table.
///
/// Changing what is shown does not rebuild the view: it *becomes* the layout
/// the data calls for, keeping its node, its sensor and its place. Which means
/// a layout can be handed state a different layout left behind, and the way to
/// find out whether that matters is to walk every pairing and look at what is
/// painted afterwards.
#[test]
fn the_explorer_survives_being_walked_through_every_pairing() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built("def transform():\n    return build_explorer(dex.ws)\n");
    let ctx = context();
    drawn(&mut ws, &ctx);

    use dex_core::refs::NodeRefs;
    let mut owned = Vec::new();
    ws.get_node(root)
        .unwrap()
        .owned_refs(&mut |uid| owned.push(uid));
    let (mode_dd, x_dd, y_dd) = (owned[0], owned[1], owned[2]);
    let choose = |ws: &mut Workspace, dd: NodeUid, index: usize| {
        ws.submit_action_dyn(Action {
            dest: dd,
            description: "choose".into(),
            body: Box::new(dex_nodes::primitives::dropdown::SetDropdownSelection { index }),
        });
        ws.process_pending();
    };

    let mut broken: Vec<String> = Vec::new();
    // Six columns in the sample: three categorical, three continuous.
    for mode in 0..2 {
        choose(&mut ws, mode_dd, mode);
        for x in 0..6 {
            for y in 0..6 {
                choose(&mut ws, x_dd, x);
                choose(&mut ws, y_dd, y);
                drawn(&mut ws, &ctx);
                let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
                let out = ctx.clone().run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |c| {
                        egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
                    },
                );
                fn walk(shape: &egui::Shape, found: &mut Option<String>) {
                    match shape {
                        egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, found)),
                        egui::Shape::Text(t) if t.galley.text().contains("draw error") => {
                            *found = Some(t.galley.text().to_owned());
                        }
                        _ => {}
                    }
                }
                let mut found = None;
                out.shapes.iter().for_each(|c| walk(&c.shape, &mut found));
                if let Some(message) = found {
                    broken.push(format!("mode {mode}, x {x}, y {y}: {message}"));
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "pairings that error:\n{}",
        broken.join("\n")
    );
}

/// What a mark stands for is what a click on it picks up.
///
/// A scatter's mark is a record and answers one row. A bar chart's mark is a
/// *category*, and every row of it was recorded at the same point — so a click
/// lands on an arbitrary member of the group, and answering with that one row
/// as though it were the bar is the wrong answer to the question the click
/// asked. The same goes for a heatmap cell, and for a tip several identical
/// lineages end on.
#[test]
fn a_mark_answers_for_everything_it_stands_for() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let checks = r#"
def transform():
    frame = Frame()

    # A point is a record, and stands for itself.
    scatter = build_plot(dex.ws, Scatter, frame=frame,
                         x="body_mass_g", y="flipper_mm")
    assert scatter.group(3) == [3], scatter.group(3)
    assert scatter.group(frame.n + 5) == [], "a row that is not there is nothing"

    # A bar is a category, and stands for every row counted into it.
    bars = build_plot(dex.ws, Bars, frame=frame, x="species")
    species = frame.values("species")
    for row in (0, 5, 17):
        want = [i for (i, v) in enumerate(species) if v == species[row]]
        assert bars.group(row) == want, (row, len(bars.group(row)), len(want))
    assert len(bars.group(0)) > 1, "the sample has more than one of each"

    # A heatmap cell is the pair, and stands for every row that crosses it.
    heat = build_plot(dex.ws, Heatmap, frame=frame, x="species", y="island")
    islands = frame.values("island")
    want = [i for i in range(frame.n)
            if species[i] == species[0] and islands[i] == islands[0]]
    assert heat.group(0) == want, (len(heat.group(0)), len(want))

    # A tip is a node of the tree, and stands for every lineage ending on it.
    lineages = {"lineage": ["A;B;C" if i % 2 else "A;B;D" for i in range(10)]}
    tree = build_plot(dex.ws, Phylogeny, lineages, x="lineage")
    assert sorted(tree.group(0)) == [0, 2, 4, 6, 8], tree.group(0)

    # And re-pointing the view forgets what it worked out about the old one.
    bars.request(SetEncoding(x="island"), None)
    want = [i for (i, v) in enumerate(islands) if v == islands[0]]
    assert bars.group(0) == want, "the group is about the columns now plotted"
    return "ok"
"#;
    assert_eq!(checked(checks), "ok");
}

/// The readout's table holds every row the mark stood for, and is drawn the
/// size of what it holds.
///
/// Both halves matter. A bar chart whose readout shows one arbitrary row of
/// four hundred answers the wrong question; and a card built to a fixed height
/// and handed one row leaves a row-shaped band of nothing under it, which reads
/// as a second, empty record rather than as slack.
#[test]
fn the_readout_holds_the_whole_group() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    \
             return build_plot(dex.ws, Bars, x='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    let wanted: usize = ask(
        &ws,
        root,
        "len(dex.snapshot.send_request(target, RowGroup(0)))",
    )
    .parse()
    .expect("a count");
    assert!(wanted > 1, "a bar of the sample stands for several rows");

    let _ = ask(
        &ws,
        root,
        "dex.snapshot.send_request(target, SetSelection(0))",
    );
    drawn(&mut ws, &ctx);
    ws.process_pending();
    drawn(&mut ws, &ctx);

    let rows = ws
        .live_ids()
        .into_iter()
        .find_map(|uid| {
            ws.get_node(uid).and_then(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<dex_nodes::primitives::table::Table>()
                    .map(|t| t.batch().num_rows())
            })
        })
        .expect("the selection became a table");
    assert_eq!(
        rows, wanted,
        "the whole category, not one arbitrary member of it"
    );

    // And the card is built around that many rows rather than around one.
    let height = |count: usize| -> f32 {
        ask(&ws, root, &format!("row_table_height({count})"))
            .parse()
            .expect("a height")
    };
    assert!(
        height(wanted) > height(1),
        "a card for {wanted} rows is taller than a card for one"
    );
}

/// A tree with more tips than there is room to name leaves them unnamed.
///
/// All of them or none, exactly as a crowded category axis is captioned: a wall
/// of overlapping names is not a denser reading of the tree, and dropping only
/// some reads as if the rest were missing rather than as if none were shown.
#[test]
fn crowded_tips_go_unnamed() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let tips = |count: usize| -> Vec<String> {
        let (mut ws, _root) = built(&format!(
            "def transform():\n    \
                 rows = {{'lineage': ['Root;Clade;Zebra %d' % i \
                 for i in range({count})]}}\n    \
                 return build_plot(dex.ws, Phylogeny, rows, x='lineage')\n"
        ));
        let ctx = context();
        drawn(&mut ws, &ctx);
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                _ => {}
            }
        }
        let mut text = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut text));
        text
    };

    let few = tips(4);
    assert!(
        few.iter().any(|s| s.starts_with("Zebra ")),
        "four tips have room for their names: {few:?}"
    );
    // Not one stump either: a truncation short enough to fit two hundred names
    // side by side is a row of initials, which looks like words and is not.
    let many = tips(200);
    assert!(
        !many.iter().any(|s| s.starts_with('Z')),
        "two hundred do not, so none of them is written: {many:?}"
    );
}

/// The join reaches through a plane to the view on it.
///
/// The join is between *views* — things that can say where they drew each row.
/// What a data explorer or a phylogeny hands back is a *plane*, which cannot;
/// the view is on it. So `link_views` given two planes has to reach through each
/// to the single view it holds (`view_of`), or it draws nothing — which is the
/// "doesn't join" the connect-views example was reported to have. The test:
/// connecting two planes still adds lines, and only when it reaches through.
#[test]
fn the_join_reaches_through_a_plane_to_its_view() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    // A minimal stand-in for examples/connect_views.py: two views, each on a
    // plane of its own, drawn side by side with the joiner over the top. The
    // joiner is handed the two planes, not the plots inside them.
    let build = |only_selected: bool| -> (Workspace, NodeUid) {
        built(&format!(
            "class Pair:\n    \
                 def __init__(self, a, b, j):\n        \
                     (self.a, self.b, self.j) = (a, b, j)\n    \
                 def owned_nodes(self):\n        \
                     return [self.j]\n    \
                 def draw(self, ctx):\n        \
                     base = ctx.constraints\n        \
                     (w, h) = (base.x.provided_value(), base.y.provided_value())\n        \
                     (x, y) = (base.pos.x, base.pos.y)\n        \
                     ctx.draw_inspectable_node(self.a, at(x, y, w / 2 - 10, h))\n        \
                     ctx.draw_inspectable_node(self.b, at(x + w / 2 + 10, y, w / 2 - 10, h))\n        \
                     ctx.draw_node(self.j, at(x, y, w, h))\n        \
                     return dex.DrawResult.Complete(\n            \
                         region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))\n\
             def transform():\n    \
                 frame = Frame()\n    \
                 a = plot_on_plane(dex.ws, build_plot(\n        \
                     dex.ws, Scatter, frame=frame, x='body_mass_g', y='flipper_mm'))\n    \
                 b = plot_on_plane(dex.ws, build_plot(\n        \
                     dex.ws, Violin, frame=frame, x='species', y='flipper_mm'))\n    \
                 j = link_views(dex.ws, a, b, only_selected={})\n    \
                 return dex.ws.insert_node_dyn(Pair(a, b, j))\n",
            if only_selected { "True" } else { "False" }
        ))
    };

    let ctx = context();
    let (mut joined, _root) = build(false);
    let with_lines = drawn(&mut joined, &ctx);
    let (mut apart, _) = build(true);
    // Nothing is selected, so the same two planes are drawn with no lines at all.
    let without = drawn(&mut apart, &ctx);
    assert!(
        with_lines > without,
        "reaching through the planes drew lines the pair alone did not: \
         {with_lines} vs {without}"
    );
}

/// `view_of` reaches a plane's view, and leaves a bare plot be.
#[test]
fn view_of_reaches_through_a_plane() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (plane_ws, plane) = built(
        "def transform():\n    return plot_on_plane(dex.ws, build_plot(\n        \
             dex.ws, Scatter, x='body_mass_g', y='flipper_mm'))\n",
    );
    assert_eq!(
        ask(
            &plane_ws,
            plane,
            "'through' if view_of(dex.snapshot, target) != target else 'same'",
        ),
        "through",
        "a plane is not a view; view_of finds the plot on it",
    );

    let (plot_ws, plot) = built(
        "def transform():\n    return build_plot(dex.ws, Scatter, \
             x='body_mass_g', y='flipper_mm')\n",
    );
    assert_eq!(
        ask(
            &plot_ws,
            plot,
            "'through' if view_of(dex.snapshot, target) != target else 'same'",
        ),
        "same",
        "a plot answers the protocol itself; view_of leaves it be",
    );
}

/// The connect-views example clones the two views wired into it.
///
/// Drawing the very same node in two places hands its widgets the same ids
/// twice, which egui paints back as a "use of widget ID" error — the clash the
/// example is reported to have when the two views are also on the surface that
/// holds the pair. Cloning is the fix: the copies have ids of their own, the
/// originals keep their place, and the join still draws.
#[test]
fn the_connect_example_clones_its_views() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    // Two views, each on a plane, the way a data explorer or a phylogeny hands
    // one back. Seated in one workspace, then wired into the example.
    let seat = |ws: &mut Workspace, script: &str, args: &[(String, ScriptValue)]| -> NodeUid {
        let (handle, actions) = WorkspaceActionHandle::buffered();
        let out = run_script(script, PRELUDE, &handle, args, GraphSnapshot::capture(ws))
            .expect("the script runs");
        let uid = match out {
            ScriptOutput::Handle(uid) => uid,
            ScriptOutput::Node(node) => ws.action_handle().insert_node_dyn(node),
            ScriptOutput::Nothing => panic!("the script returns something"),
        };
        drop(handle);
        for action in actions.try_iter() {
            ws.submit_action_dyn(action);
        }
        ws.process_pending();
        uid
    };

    let left = seat(
        &mut ws,
        "def transform():\n    return plot_on_plane(dex.ws, build_plot(\n        \
             dex.ws, Scatter, x='body_mass_g', y='flipper_mm'))\n",
        &[],
    );
    let right = seat(
        &mut ws,
        "def transform():\n    return plot_on_plane(dex.ws, build_plot(\n        \
             dex.ws, Violin, x='species', y='flipper_mm'))\n",
        &[],
    );
    let before = ws.live_ids().len();

    let connected = seat(
        &mut ws,
        include_str!("../../../examples/connect_views.py"),
        &[
            ("thisView".to_owned(), ScriptValue::Node(left)),
            ("thatView".to_owned(), ScriptValue::Node(right)),
        ],
    );

    // The originals are left in place, and the clones were seated alongside.
    assert!(
        ws.get_node(left).is_some() && ws.get_node(right).is_some(),
        "the two wired views were cloned, not consumed"
    );
    assert!(
        ws.live_ids().len() > before,
        "the clones joined the workspace beside the originals"
    );

    ws.set_root(connected);
    ws.process_pending();
    let ctx = context();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut texts: Vec<String> = Vec::new();
    for _ in 0..5 {
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        ws.process_pending();
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                _ => {}
            }
        }
        texts.clear();
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
    }
    assert!(
        !texts.iter().any(|s| s.contains("use of") && s.contains("ID")),
        "no id clash: the views are copies with ids of their own, not the \
         originals drawn a second time"
    );
}

/// Every layout's readout works, and works the same way.
///
/// The readout is spine code — the table is sliced, seated and sized in one
/// place — but each layout decides what a mark stands for and where it was
/// drawn, and either of those going wrong shows up only here. So this walks the
/// lot: select a row, draw, and check that what was seated is a real table
/// holding exactly the rows the view says the mark stood for, with nothing on
/// the plane complaining.
#[test]
fn every_layout_shows_the_records_behind_a_mark() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    // `x`/`y` chosen per layout the way the explorer would choose them, so each
    // is asked for the picture it is actually for.
    let layouts = [
        ("Scatter", "x='body_mass_g', y='flipper_mm', color='species'"),
        ("Bars", "x='species'"),
        ("Strip", "x='species', y='body_mass_g'"),
        ("Violin", "x='species', y='body_mass_g'"),
        ("Heatmap", "x='species', y='island'"),
        ("Phylogeny", "x='species', color='island'"),
        ("Circos", "x='species', tracks=['body_mass_g']"),
        ("Scatter3D", "x='body_mass_g', y='flipper_mm', z='bill_length_mm'"),
    ];
    let mut broken = Vec::new();
    for (layout, encoding) in layouts {
        let (mut ws, root) = built(&format!(
            "def transform():\n    \
                 return build_plot(dex.ws, {layout}, {encoding})\n"
        ));
        let ctx = context();
        drawn(&mut ws, &ctx);
        let _ = ask(
            &ws,
            root,
            "dex.snapshot.send_request(target, SetSelection(0))",
        );
        drawn(&mut ws, &ctx);
        ws.process_pending();
        let painted = drawn(&mut ws, &ctx);

        let wanted: usize = ask(
            &ws,
            root,
            "len(dex.snapshot.send_request(target, RowGroup(0)))",
        )
        .parse()
        .unwrap_or(0);
        let rows = ws.live_ids().into_iter().find_map(|uid| {
            ws.get_node(uid).and_then(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<dex_nodes::primitives::table::Table>()
                    .map(|t| t.batch().num_rows())
            })
        });
        if wanted == 0 {
            broken.push(format!("{layout}: the mark stands for nothing"));
        } else if rows != Some(wanted) {
            broken.push(format!(
                "{layout}: the readout holds {rows:?} rows, the mark stands for {wanted}"
            ));
        }
        if painted < 20 {
            broken.push(format!("{layout}: barely drew ({painted} shapes)"));
        }
    }
    assert!(broken.is_empty(), "readouts that misbehave:\n{}", broken.join("\n"));
}


/// A readout table shares its view's layer, and must not clash with it.
///
/// The view's sensor claims the layer it is drawn on so egui can tell the
/// pointer is over it; a `Table` node hosted in the same readout is a second
/// thing that would claim the same layer, and egui hands both the layer's own
/// `move` widget id at different rects — the "First/Second use of widget ID"
/// error painted right into the frame. One area per layer per frame is what
/// stops it. A bar's selection is the case that shows it: the table is tall
/// enough to want a scrollbar, drawn on the same layer as the sensor.
#[test]
fn a_readout_table_does_not_clash_with_its_view() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(
        "def transform():\n    return build_plot(dex.ws, Bars, x='species')\n",
    );
    let ctx = context();
    drawn(&mut ws, &ctx);
    let _ = ask(&ws, root, "dex.snapshot.send_request(target, SetSelection(3))");

    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut texts: Vec<String> = Vec::new();
    for _ in 0..5 {
        let out = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: Vec::new(),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            },
        );
        ws.process_pending();
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                _ => {}
            }
        }
        texts.clear();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut texts);
        }
    }
    let clashes: Vec<&String> = texts
        .iter()
        .filter(|s| s.contains("use of") && s.contains("ID"))
        .collect();
    assert!(clashes.is_empty(), "an egui id clash was painted: {clashes:?}");
}
