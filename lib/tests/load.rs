use std::fs::File;
use lib::load::{open_csv, sniff_file};

#[test]
fn test_sniff() {
    let file = File::open("test2.csv").unwrap();
    let format = sniff_file(file.try_clone().unwrap()).unwrap();
    println!("{format:#?}");

    // let _batch = open_csv(file.try_clone().unwrap(), file.try_clone().unwrap(), format).unwrap();
}
