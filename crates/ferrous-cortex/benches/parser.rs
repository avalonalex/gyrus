use criterion::{Criterion, criterion_group, criterion_main};
use ferrous_cortex::parse;
use std::hint::black_box;

fn bench_parse_simple(c: &mut Criterion) {
    let source = "+++++[>+++++<-]>.";

    c.bench_function("parse_simple", |b| {
        b.iter(|| {
            parse(black_box(source)).unwrap();
        });
    });
}

fn bench_parse_nested_loops(c: &mut Criterion) {
    let source = "+++[>+++[>+++[>+++<-]<-]<-]";

    c.bench_function("parse_nested_loops", |b| {
        b.iter(|| {
            parse(black_box(source)).unwrap();
        });
    });
}

fn bench_parse_long_program(c: &mut Criterion) {
    // Repeat a pattern many times to create a longer program
    let source = "+++++[>+++++<-]>.".repeat(100);

    c.bench_function("parse_long_program", |b| {
        b.iter(|| {
            parse(black_box(&source)).unwrap();
        });
    });
}

fn bench_parse_hello_world(c: &mut Criterion) {
    let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";

    c.bench_function("parse_hello_world", |b| {
        b.iter(|| {
            parse(black_box(source)).unwrap();
        });
    });
}

fn bench_parse_with_comments(c: &mut Criterion) {
    let source = r"
        * This is a comment
        +++++     * Add 5
        [         * Loop
          >++     * Move and add 2
          <-      * Move back and subtract
        ]         * End loop
        >.        * Output
    ";

    c.bench_function("parse_with_comments", |b| {
        b.iter(|| {
            parse(black_box(source)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_parse_simple,
    bench_parse_nested_loops,
    bench_parse_long_program,
    bench_parse_hello_world,
    bench_parse_with_comments
);
criterion_main!(benches);
