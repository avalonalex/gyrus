use criterion::{Criterion, criterion_group, criterion_main};
use ferrous_cortex::{ExecutionConfig, interpret_with_io, io::StringIo, parse};
use std::hint::black_box;

fn bench_simple_arithmetic(c: &mut Criterion) {
    let source = "+++++[>++++[>++<-]<-]";
    let instructions = parse(source).unwrap();

    c.bench_function("simple_arithmetic", |b| {
        b.iter(|| {
            let config = ExecutionConfig::default();
            let mut input = StringIo::empty();
            let mut output = StringIo::empty();
            interpret_with_io(
                black_box(&instructions),
                config,
                &mut input,
                &mut output,
                None,
            )
            .unwrap();
        });
    });
}

fn bench_nested_loops(c: &mut Criterion) {
    let source = "+++[>+++[>+++[>+++<-]<-]<-]";
    let instructions = parse(source).unwrap();

    c.bench_function("nested_loops", |b| {
        b.iter(|| {
            let config = ExecutionConfig::default();
            let mut input = StringIo::empty();
            let mut output = StringIo::empty();
            interpret_with_io(
                black_box(&instructions),
                config,
                &mut input,
                &mut output,
                None,
            )
            .unwrap();
        });
    });
}

fn bench_pointer_movement(c: &mut Criterion) {
    let source = ">>>>>>>>>><<<<<<<<<<>>>>>>>>>><<<<<<<<<<";
    let instructions = parse(source).unwrap();

    c.bench_function("pointer_movement", |b| {
        b.iter(|| {
            let config = ExecutionConfig::default();
            let mut input = StringIo::empty();
            let mut output = StringIo::empty();
            interpret_with_io(
                black_box(&instructions),
                config,
                &mut input,
                &mut output,
                None,
            )
            .unwrap();
        });
    });
}

fn bench_io_operations(c: &mut Criterion) {
    let source = ",[.,]"; // Echo input
    let instructions = parse(source).unwrap();
    let input_data = "Hello, World!";

    c.bench_function("io_operations", |b| {
        b.iter(|| {
            let config = ExecutionConfig::default();
            let mut input = StringIo::new(black_box(input_data));
            let mut output = StringIo::empty();
            interpret_with_io(
                black_box(&instructions),
                config,
                &mut input,
                &mut output,
                None,
            )
            .unwrap();
        });
    });
}

fn bench_hello_world(c: &mut Criterion) {
    // Classic Hello World program
    let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
    let instructions = parse(source).unwrap();

    c.bench_function("hello_world", |b| {
        b.iter(|| {
            let config = ExecutionConfig::default();
            let mut input = StringIo::empty();
            let mut output = StringIo::empty();
            interpret_with_io(
                black_box(&instructions),
                config,
                &mut input,
                &mut output,
                None,
            )
            .unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_simple_arithmetic,
    bench_nested_loops,
    bench_pointer_movement,
    bench_io_operations,
    bench_hello_world
);
criterion_main!(benches);
