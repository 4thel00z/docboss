use criterion::{criterion_group, criterion_main, Criterion};

fn bench(c: &mut Criterion) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            Some((
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).ok()?,
            ))
        })
        .collect();
    files.sort();
    for (name, bytes) in files {
        c.bench_function(&format!("cfb/walk/{name}"), |b| {
            b.iter(|| {
                let file = docboss_cfb::CompoundFile::parse(&bytes).unwrap();
                file.walk()
                    .iter()
                    .map(|(path, _)| file.open_stream(path).map_or(0, |s| s.len()))
                    .sum::<usize>()
            })
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
