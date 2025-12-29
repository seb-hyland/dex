use std::{fs::File, io::Cursor};
use criterion::{Criterion, criterion_group, criterion_main};
use lib::load::{open_csv, sniff_file};
use memmap2::{Advice, Mmap};

fn bench_std_file(c: &mut Criterion) {
    c.bench_function("std_file", |b| {
        b.iter(|| {
            let file = File::open("test.csv").unwrap();
            let format = sniff_file(file.try_clone().unwrap()).unwrap();
            
            let _batch =
                open_csv(file.try_clone().unwrap(), file.try_clone().unwrap(), format).unwrap();
        });
    });
}

// fn bench_mmap(c: &mut Criterion) {
//     c.bench_function("memmap", |b| {
//         b.iter(|| {
//             let file = File::open("test.csv").unwrap();
//             let mmap = unsafe { Mmap::map(&file) }.unwrap();
//             mmap.advise(Advice::Sequential).unwrap();
//             mmap.advise(Advice::WillNeed).unwrap();
//             let format = sniff_file(Cursor::new(&mmap)).unwrap();

//             let _batch = open_csv(&mmap[..], &mmap[..], format).unwrap();
//         })
//     });
// }

criterion_group!(benches,
    bench_std_file,
    // bench_mmap
);
criterion_main!(benches);
