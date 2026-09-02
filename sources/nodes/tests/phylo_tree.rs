//! Exercises `examples/phylo_tree.py` and `examples/lineage_table.py`: a
//! taxonomic tree drawn on a canvas from a column of lineage strings.
//!
//! Two things are worth pinning. The tree building is pure and is where the
//! subtle bugs live — two genera of the same name under different families are
//! two nodes, not one, and homonyms across the tree of life are the rule. And
//! the layout has to survive not knowing how wide a name is until it has drawn
//! one, which it does by throwing the first frame away.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{CanvasChildren, CanvasLayerNodes, Layer};
use dex_nodes::layouts::canvas::nodes::CanvasItemBounds;
use dex_nodes::primitives::table::Table;
use dex_nodes::scripting::{ScriptOutput, run_script};
use pyo3::prelude::*;
use pyo3::types::PyDict;

const PHYLO: &str = include_str!("../../../examples/phylo_tree.py");
const LINEAGE_TABLE: &str = include_str!("../../../examples/lineage_table.py");
const SCREEN: egui::Vec2 = egui::vec2(1100.0, 760.0);

fn load<'py>(py: Python<'py>, source: &str) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
        .unwrap();
    let src = std::ffi::CString::new(source).unwrap();
    py.run(src.as_c_str(), Some(&globals), Some(&globals))
        .expect("example module runs");
    globals
}

fn eval<T>(py: Python<'_>, globals: &Bound<'_, PyDict>, expr: &str) -> T
where
    T: for<'a, 'py> pyo3::FromPyObject<'a, 'py>,
{
    let code = std::ffi::CString::new(expr).unwrap();
    let value = py
        .eval(code.as_c_str(), Some(globals), None)
        .unwrap_or_else(|e| panic!("`{expr}` evaluates: {e}"));
    match value.extract() {
        Ok(out) => out,
        Err(_) => panic!("`{expr}` has the expected type"),
    }
}

/// Whether the interpreter can build Arrow columns. Without it a table cannot
/// be made from a script at all, which is what the global environment setting
/// is for — so those tests say why they are skipping rather than failing.
fn has_pyarrow() -> bool {
    dex_nodes::scripting::init_python();
    Python::attach(|py| py.import("pyarrow").is_ok())
}

/// Run a transform into `ws`, exactly as a lambda would.
fn run(ws: &mut Workspace, source: &str) -> ScriptOutput {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let out = run_script(source, "", &handle, &[], graph).unwrap_or_else(|e| panic!("{e}"));
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    out
}

/// A lineage splits into its ranks, and stops where the data stops saying.
#[test]
fn a_lineage_splits_into_ranked_names() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load(py, PHYLO);

        let parsed: Vec<(String, String)> = eval(
            py,
            &globals,
            "split_lineage('d__Bacteria;p__Bacillota;g__Streptococcus')",
        );
        assert_eq!(
            parsed,
            [
                ("d".to_owned(), "Bacteria".to_owned()),
                ("p".to_owned(), "Bacillota".to_owned()),
                ("g".to_owned(), "Streptococcus".to_owned()),
            ]
        );

        // A field with no prefix is still a name: not every tool writes them.
        let bare: Vec<(String, String)> = eval(py, &globals, "split_lineage('Bacteria;Bacillota')");
        assert_eq!(
            bare,
            [
                (String::new(), "Bacteria".to_owned()),
                (String::new(), "Bacillota".to_owned()),
            ]
        );

        // An unassigned rank ends the lineage rather than becoming a taxon of
        // its own — otherwise every unclassified read joins the same fake node.
        let cut: Vec<(String, String)> = eval(
            py,
            &globals,
            "split_lineage('d__Bacteria;p__Bacillota;c__unclassified;o__Nope')",
        );
        assert_eq!(cut.len(), 2, "the lineage stops at the unassigned rank");

        // Trailing empties, which a truncated lineage string is full of.
        let padded: Vec<(String, String)> =
            eval(py, &globals, "split_lineage('d__Bacteria;p__Bacillota;;;')");
        assert_eq!(padded.len(), 2);
    });
}

/// Lineages sharing a prefix share nodes — and names that merely look alike do
/// not. A genus is identified by its whole path, because homonyms across the
/// tree of life are the rule rather than the exception.
#[test]
fn a_shared_prefix_is_a_shared_node_and_a_homonym_is_not() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load(py, PHYLO);

        // Two species of one genus: one domain, one phylum, one genus, two tips.
        py.run(
            c"nodes, roots = build_tree([\
                'd__B;p__P;g__G;s__G one',\
                'd__B;p__P;g__G;s__G two',\
            ])",
            Some(&globals),
            Some(&globals),
        )
        .unwrap();
        let (total, tips): (usize, usize) = eval(
            py,
            &globals,
            "(len(nodes), len([k for k in nodes if not nodes[k]['children']]))",
        );
        assert_eq!(total, 5, "domain, phylum, genus and two species");
        assert_eq!(tips, 2, "the two species are the only leaves");

        // The same genus name under two different phyla is two nodes.
        py.run(
            c"nodes, roots = build_tree([\
                'd__B;p__One;g__Same;s__A',\
                'd__B;p__Two;g__Same;s__B',\
            ])",
            Some(&globals),
            Some(&globals),
        )
        .unwrap();
        let same: usize = eval(
            py,
            &globals,
            "len([k for k in nodes if nodes[k]['name'] == 'Same'])",
        );
        assert_eq!(
            same, 2,
            "a homonym under a different parent is a different taxon"
        );

        // Weight accumulates up the lineage, so a parent carries its subtree.
        py.run(
            c"nodes, roots = build_tree(\
                ['d__B;p__P;s__One', 'd__B;p__P;s__Two'], [3.0, 4.0])",
            Some(&globals),
            Some(&globals),
        )
        .unwrap();
        let root_weight: f64 = eval(py, &globals, "nodes[('B',)]['weight']");
        assert_eq!(root_weight, 7.0, "the domain carries everything beneath it");
    });
}

/// Leaves stack in the order they are walked and every parent sits on the
/// midpoint of its children, which is what makes a dendrogram readable.
#[test]
fn parents_sit_centred_on_their_children() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load(py, PHYLO);
        py.run(
            c"nodes, roots = build_tree([\
                'd__B;p__P;g__G1;s__A',\
                'd__B;p__P;g__G1;s__B',\
                'd__B;p__P;g__G2;s__C',\
            ])\nrows = assign_rows(nodes, roots)",
            Some(&globals),
            Some(&globals),
        )
        .unwrap();

        let leaves: Vec<f64> = eval(
            py,
            &globals,
            "[rows[('B','P','G1','A')], rows[('B','P','G1','B')], rows[('B','P','G2','C')]]",
        );
        assert_eq!(
            leaves,
            [0.0, 1.0, 2.0],
            "leaves stack one per row, in order"
        );

        let g1: f64 = eval(py, &globals, "rows[('B','P','G1')]");
        assert_eq!(g1, 0.5, "a genus sits between its two species");
        // Centred on its children, not on the leaves under it: the phylum sits
        // between G1 at 0.5 and G2 at 2.0, which is 1.25 and not the 1.0 the
        // leaf span would give. That is the rule that keeps an unbranching run
        // straight — a node with one child sits exactly on it.
        let phylum: f64 = eval(py, &globals, "rows[('B','P')]");
        assert_eq!(phylum, 1.25, "the phylum is the mean of its two genera");
        let root: f64 = eval(py, &globals, "rows[('B',)]");
        assert_eq!(root, phylum, "and the domain, with one child, sits on it");
    });
}

/// A column is as wide as the widest name in it, so a name never runs into the
/// dots of the column after it.
#[test]
fn a_column_is_as_wide_as_the_name_it_has_to_hold() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load(py, PHYLO);
        let (min_column, pad, tree_x): (f64, f64, f64) =
            eval(py, &globals, "(MIN_COLUMN, COLUMN_PAD, TREE_X)");

        // A narrow column still takes its minimum; a wide one takes its width.
        let xs: Vec<f64> = eval(py, &globals, "column_x([10.0, 400.0, 10.0])");
        assert_eq!(
            xs[0], tree_x,
            "the first column starts at the tree's origin"
        );
        assert_eq!(
            xs[1] - xs[0],
            min_column,
            "a narrow name does not squeeze the column below its minimum"
        );
        assert_eq!(
            xs[2] - xs[1],
            400.0 + pad,
            "and a wide one widens it, with room before the next column"
        );

        // Dots grow with what they carry, by area rather than radius.
        let (small, big): (f64, f64) = eval(
            py,
            &globals,
            "(dot_radius(0.0, 100.0), dot_radius(100.0, 100.0))",
        );
        let (dot_min, dot_max): (f64, f64) = eval(py, &globals, "(DOT_MIN, DOT_MAX)");
        assert_eq!(small, dot_min);
        assert_eq!(big, dot_max);
        let half: f64 = eval(py, &globals, "dot_radius(25.0, 100.0)");
        assert!(
            (half - (dot_min + (dot_max - dot_min) * 0.5)).abs() < 1e-9,
            "a quarter of the reads is half the extra radius, so area tracks the count"
        );
        // Nothing to divide by is not a crash.
        let none: f64 = eval(py, &globals, "dot_radius(5.0, 0.0)");
        assert_eq!(none, dot_min);
    });
}

/// The transform builds a canvas: a dot per taxon, the lines and axis beneath
/// them, the readout over. Run with nothing wired, so it draws its own sample.
#[test]
fn the_transform_builds_a_canvas_of_the_tree() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = match run(&mut ws, PHYLO) {
        ScriptOutput::Handle(uid) => uid,
        _ => panic!("the tree returns the canvas it built"),
    };

    let taxa: usize = Python::attach(|py| {
        let globals = load(py, PHYLO);
        eval(py, &globals, "len(build_tree(SAMPLE_LINEAGES)[0])")
    });
    assert!(taxa > 20, "the sample is a tree worth drawing: {taxa} taxa");

    let items = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(items.len(), taxa, "one dot per taxon");
    for &item in &items {
        assert_eq!(
            ws.send_request(item, Inspectable),
            Some(false),
            "a dot is content, not a handle"
        );
        assert!(
            ws.send_request(item, CanvasItemBounds).is_some(),
            "and sits somewhere on the plane"
        );
    }

    let backgrounds = ws
        .send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Background,
            },
        )
        .unwrap_or_default();
    assert_eq!(backgrounds.len(), 2, "the rank axis and the tree lines");
    let foregrounds = ws
        .send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Foreground,
            },
        )
        .unwrap_or_default();
    assert_eq!(foregrounds.len(), 1, "and the readout over them");
}

/// The tree draws through real frames. The first pass measures the names and
/// throws itself away, so what matters is that the second one paints a dot per
/// taxon and a name beside each.
#[test]
fn the_tree_measures_its_names_and_then_draws_them() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = match run(&mut ws, PHYLO) {
        ScriptOutput::Handle(uid) => uid,
        _ => panic!("the tree returns the canvas it built"),
    };
    ws.set_root(canvas);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut output = None;
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let ws = &mut ws;
        output = Some(egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        }));
    }
    let output = output.expect("the canvas drew");

    let (taxa, headings, headings_text): (usize, usize, Vec<String>) = Python::attach(|py| {
        let globals = load(py, PHYLO);
        eval(
            py,
            &globals,
            "(lambda n: (len(n), len(rank_labels(n)), rank_labels(n)))\
             (build_tree(SAMPLE_LINEAGES)[0])",
        )
    });

    let circles = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Circle(_)))
        .count();
    assert_eq!(circles, taxa, "a dot for each of {taxa} taxa");

    let painted: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    let texts = painted.len();
    assert_eq!(
        texts,
        taxa + headings,
        "a name beside each dot, and a caption over each of {headings} rank columns"
    );

    // That the names drew at all is what proves the measuring pass ended: a
    // frame still measuring returns before drawing any of them.
    assert!(
        painted.contains(&"Streptococcus pneumoniae".to_owned()),
        "the longest name in the sample is among them"
    );

    // A bracket per parent: a stub out, one vertical spanning every child, and
    // a stub into each — so two segments plus one per child. An elbow per child
    // would be three each, laying identical verticals over one another and
    // thickening the line by overdraw.
    let branches: usize = Python::attach(|py| {
        let globals = load(py, PHYLO);
        eval(
            py,
            &globals,
            "sum(2 + len(n['children']) for n in build_tree(SAMPLE_LINEAGES)[0].values() \
             if n['children'])",
        )
    });
    let lines: Vec<egui::Rect> = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Path(_)))
        .map(|c| c.shape.visual_bounding_rect())
        .collect();
    assert_eq!(
        lines.len(),
        branches,
        "the tree is drawn as brackets, not elbows"
    );

    // A name is boxed, and the box has a branch on neither side of it: the
    // stub out of a node starts past its name rather than at its dot, because
    // a branch running behind a label is a branch cut in half.
    let names: Vec<egui::Rect> = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            // The rank headings sit above the tree; the names are the rest.
            egui::Shape::Text(t) if !headings_text.contains(&t.galley.text().to_owned()) => {
                Some(c.shape.visual_bounding_rect())
            }
            _ => None,
        })
        .collect();
    assert_eq!(names.len(), taxa, "every taxon's name reached the screen");

    for name in &names {
        // Shrunk a little: a stub ending one point clear of a box is clear of
        // it, and float edges are not what this is asking about.
        let text = name.shrink(1.5);
        for line in &lines {
            assert!(
                !line.intersects(text),
                "a branch at {line:?} runs through a name at {name:?}"
            );
        }
    }

    // One box behind each name, plus the shaded rank bands.
    let bands = headings.div_ceil(2);
    let rects = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Rect(_)))
        // Not the panel egui paints the whole thing onto, which is the only
        // rect as wide as the screen.
        .filter(|c| c.shape.visual_bounding_rect().width() < SCREEN.x)
        .count();
    assert_eq!(
        rects,
        taxa + bands,
        "a box behind each of {taxa} names, and {bands} shaded bands"
    );
}

/// A transform can hand back a table. Anything carrying Arrow columns becomes
/// one, which is what lets a script build the input the tree reads.
#[test]
fn a_script_can_return_a_table() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    let node = match run(&mut ws, LINEAGE_TABLE) {
        ScriptOutput::Node(node) => node,
        _ => panic!("the generator returns a table"),
    };
    let table = (*node)
        .as_any_ref()
        .downcast_ref::<Table>()
        .expect("a pyarrow table became a Table node");

    let batch = table.batch();
    let schema = batch.schema();
    let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert!(
        names.contains(&"lineage") && names.contains(&"reads"),
        "the columns survived the crossing: {names:?}"
    );
    assert!(batch.num_rows() > 10, "and so did the rows");
}

/// The tree reads the table the generator built: end to end, the way the two
/// examples are meant to be wired together.
#[test]
fn the_tree_draws_the_table_the_generator_built() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let table = load(py, LINEAGE_TABLE);
        let built = py
            .eval(c"transform()", Some(&table), None)
            .expect("the generator builds a table");

        let tree = load(py, PHYLO);
        tree.set_item("t", built).unwrap();
        let (lineages, weights): (usize, usize) = eval(
            py,
            &tree,
            "(lambda pair: (len(pair[0]), len(pair[1])))(lineage_column(t))",
        );
        assert!(
            lineages > 10,
            "the tree read the lineage column: {lineages}"
        );
        assert_eq!(
            weights, lineages,
            "and the read counts alongside them, one per lineage"
        );

        // Those lineages build the tree the generator's sample describes.
        let (taxa, tips): (usize, usize) = eval(
            py,
            &tree,
            "(lambda n: (len(n), len([k for k in n if not n[k]['children']])))\
             (build_tree(*lineage_column(t))[0])",
        );
        assert!(
            taxa > tips,
            "the tree branches: {taxa} taxa over {tips} tips"
        );
        assert_eq!(
            tips, lineages,
            "and every row is a tip, since every row is a full lineage"
        );
    });
}
