use lib::{compute::python, load::load_delimited_file};

#[test]
fn test_transform() {
    let batch = load_delimited_file("tests/test.csv").unwrap();
    let code = r#"
import pyarrow as pa
import polars as pl

def transform(batch):
    df = pl.from_arrow(batch)
    subset_df = df.filter(pl.col("Country") == "Nepal")
    return subset_df.to_arrow().combine_chunks().to_batches()[0]
"#;
    let res = python::apply_transform(
        vec![batch],
        code,
        Some("/Users/seb-hyland/Documents/dex/lib/tests/venv/lib/python3.14/site-packages"),
    )
    .unwrap();
    println!("Transformed: {res:#?}");
}
