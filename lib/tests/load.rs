use csv_nose::{SampleSize, Sniffer};
use lib::load::load_delimited_file;

#[test]
fn test_sniff() {
    let batch = load_delimited_file("test.csv").unwrap();
    println!("{batch:#?}");
}
