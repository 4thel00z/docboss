use criterion::{criterion_group, criterion_main, Criterion};

fn bench(c: &mut Criterion) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "doc"))
        .filter_map(|e| {
            Some((
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).ok()?,
            ))
        })
        .collect();
    files.sort();
    for (name, bytes) in files {
        c.bench_function(&format!("doc/read/{name}"), |b| {
            b.iter(|| docboss_doc::read(&bytes).map(|d| d.sections.len()))
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
