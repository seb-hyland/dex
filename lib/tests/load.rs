use lib::load::load_delimited_file;

#[test]
fn test_sniff() {
    let batch = load_delimited_file("test2.csv").unwrap();
    println!("{batch:#?}");
}
