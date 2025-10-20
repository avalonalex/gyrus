# FerrousCortex

An industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust.

## Features

- Fast and efficient BrainFuck interpreter
- Proper error handling with descriptive error messages
- Support for all 8 BrainFuck commands: `><+-.,[]`
- Nested loop support
- 30,000 bytes of memory (standard BF specification)
- Command-line interface

## Installation

### Prerequisites

- Rust 1.85+ with edition 2024 support
- Cargo (comes with Rust)

### Building from Source

```bash
git clone <repository-url>
cd FerrousCortex
cargo build --release
```

The compiled binary will be available at `target/release/ferrous-cortex`.

## Usage

### Running a BrainFuck Program

```bash
cargo run -- <path-to-bf-file>

# Or using the compiled binary
./target/release/ferrous-cortex <path-to-bf-file>
```

### Examples

Run the included example programs:

```bash
# Simple program that prints 'H'
cargo run -- examples/simple.bf

# Classic Hello World
cargo run -- examples/hello_world.bf
```

### Command-Line Options

```bash
ferrous-cortex --help
```

## BrainFuck Language Reference

| Command | Description |
|---------|-------------|
| `>`     | Move pointer right |
| `<`     | Move pointer left |
| `+`     | Increment value at pointer |
| `-`     | Decrement value at pointer |
| `.`     | Output byte at pointer |
| `,`     | Input byte to pointer |
| `[`     | Jump forward past matching `]` if value at pointer is 0 |
| `]`     | Jump back to matching `[` if value at pointer is non-zero |

Any other characters are treated as comments and ignored.

## Development

### Running Tests

```bash
cargo test
```

### Development Build

```bash
cargo build
```

### Running with Cargo

```bash
cargo run -- path/to/your/program.bf
```

## Project Structure

```
FerrousCortex/
├── src/
│   ├── main.rs          # CLI interface and entry point
│   └── bf.rs            # Parser and interpreter core
├── examples/            # Example BrainFuck programs
│   ├── hello_world.bf
│   └── simple.bf
├── Cargo.toml
└── README.md
```

## Roadmap

- [ ] Visual TUI debugger with breakpoints
- [ ] Step-by-step execution
- [ ] Memory visualization
- [ ] Performance optimizations (instruction fusion, loop detection)
- [ ] JIT/AOT compiler backend
- [ ] Better error messages with source context
- [ ] REPL mode

## References

- [The BrainFuck Programming Language](https://www.muppetlabs.com/~breadbox/bf/) - Comprehensive guide and reference by Brian Raiter

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]
