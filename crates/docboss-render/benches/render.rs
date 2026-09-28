//! Layout and rendering throughput on a synthetic report of about 100
//! pages: headings, justified text, lists, tables, images and footnotes.

mod sample;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn benches(c: &mut Criterion) {
    let document = sample::document(115);
    let fonts = docboss_layout::fonts_for(&document);
    let layout = docboss_layout::layout(&document, &fonts);
    let pages = layout.pages.len() as u64;
    let mut group = c.benchmark_group("report");
    group.sample_size(10);
    group.throughput(Throughput::Elements(pages));
    group.bench_function("layout", |b| {
        b.iter(|| docboss_layout::layout(&document, &fonts))
    });
    group.bench_function("render_sequential_1x", |b| {
        b.iter(|| {
            let mut renderer = docboss_render::Renderer::new();
            (0..layout.pages.len()).for_each(|i| drop(renderer.render(&layout, i, 1.0)));
        })
    });
    group.bench_function("render_parallel_1x", |b| {
        b.iter(|| docboss_render::render_pages(&layout, 1.0))
    });
    group.bench_function("render_png_parallel_2x", |b| {
        b.iter(|| {
            docboss_render::render_pages(&layout, 2.0)
                .into_iter()
                .filter_map(Result::ok)
                .map(|p| {
                    p.encode(docboss_render::Format::Png)
                        .map(|v| v.len())
                        .unwrap_or(0)
                })
                .sum::<usize>()
        })
    });
    group.finish();
}

criterion_group!(render, benches);
criterion_main!(render);
