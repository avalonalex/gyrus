# Syntax Highlighter Design

## Overview

Add a syntax highlighter for BrainFuck code that can be reused across multiple tools (CLI, TUI, debugger, IDE plugins).

## Architecture

### Core Highlighter (Library)

**Location**: `crates/ferrous-cortex/src/syntax.rs` (new module)

**Purpose**:
- Reusable syntax highlighting logic
- Format-agnostic (can output ANSI, HTML, plain text, etc.)
- Works with source code and parsed AST

**Key Features**:
1. Highlight different instruction types with distinct colors/styles
2. Show comments distinctly
3. Visualize loop nesting depth
4. Optional line numbers
5. Multiple output formats

### View Command (Tool CLI)

**Location**: `ferrous-cortex-tool view` subcommand

**Purpose**:
- Pretty-print BF programs to console with syntax highlighting
- Useful for code review, documentation, debugging

## Design Details

### Color Scheme

**Instruction Categories**:
- **Movement** (`<>`) - Blue (data flow)
- **Arithmetic** (`+-`) - Green (computation)
- **I/O** (`,.`) - Yellow (interaction)
- **Loops** (`[]`) - Magenta/Bold (control flow)
- **Comments** - Gray/Dim (documentation)

**Additional Visual Elements**:
- Line numbers - Dim gray
- Nesting indicators - Colored background or symbols
- Whitespace - Preserved but not highlighted

### Output Formats

1. **ANSI Terminal** (default) - Colored output for console
2. **Plain** - No colors, just formatted text
3. **HTML** (future) - For documentation generation
4. **JSON** (future) - For IDE integrations

### Highlighter API

```rust
// Core highlighting struct
pub struct SyntaxHighlighter {
    theme: ColorTheme,
    show_line_numbers: bool,
    show_nesting: bool,
}

// Color theme configuration
pub struct ColorTheme {
    movement: Color,
    arithmetic: Color,
    io: Color,
    loops: Color,
    comments: Color,
    line_numbers: Color,
}

// Highlighted output
pub struct HighlightedCode {
    lines: Vec<HighlightedLine>,
}

pub struct HighlightedLine {
    number: Option<usize>,
    spans: Vec<HighlightedSpan>,
}

pub struct HighlightedSpan {
    text: String,
    style: SpanStyle,
}

pub enum SpanStyle {
    Movement,
    Arithmetic,
    Io,
    LoopStart(usize), // nesting depth
    LoopEnd(usize),
    Comment,
    Whitespace,
}

// Main API
impl SyntaxHighlighter {
    pub fn new() -> Self;
    pub fn with_theme(theme: ColorTheme) -> Self;

    pub fn highlight(&self, source: &str) -> HighlightedCode;
    pub fn highlight_with_ast(&self, source: &str, instructions: &[Instruction]) -> HighlightedCode;
}

impl HighlightedCode {
    pub fn to_ansi(&self) -> String;
    pub fn to_plain(&self) -> String;
    pub fn to_html(&self) -> String; // future
}
```

## View Command Specification

### Usage

```bash
# Basic usage - pretty print with colors
ferrous-cortex-tool view program.bf

# Show line numbers
ferrous-cortex-tool view program.bf --line-numbers

# Show nesting depth indicators
ferrous-cortex-tool view program.bf --show-nesting

# Plain output (no colors)
ferrous-cortex-tool view program.bf --plain

# Custom theme
ferrous-cortex-tool view program.bf --theme dark
ferrous-cortex-tool view program.bf --theme light

# With context (show N lines around loops)
ferrous-cortex-tool view program.bf --context 3

# Output to file (HTML)
ferrous-cortex-tool view program.bf --output program.html --format html
```

### Examples

**Input** (`hello_world.bf`):
```brainfuck
* Classic Hello World program
++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.
```

**Output** (with colors in terminal):
```
1 │ * Classic Hello World program
2 │ ++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.
    ^^^^^^^^│     │                  │                      ││
    green   │     │                  │                      │└─ yellow (I/O)
    (arith) │     │                  │                      └─ yellow (I/O)
            │     │                  └─ magenta (loop end, depth 2)
            │     └─ magenta (loop start, depth 2)
            └─ magenta (loop start, depth 1)
```

With `--show-nesting`:
```
1 │ * Classic Hello World program
2 │ ++++++++[          depth 1
3 │         >++++[     depth 2
4 │              >++>+++>+++>+<<<<-
5 │              ]     depth 2 end
6 │         >+>+>->>+[<]<-
7 │         ]          depth 1 end
8 │ >>.>---.+++++++..+++.
```

## Implementation Plan

### Phase 1: Core Highlighter ✅

1. Create `src/syntax.rs` module
2. Implement basic highlighting logic
3. Add ANSI color output
4. Add tests

### Phase 2: View Command ✅

1. Add `view` subcommand to `ferrous-cortex-tool`
2. Integrate syntax highlighter
3. Add command-line options
4. Test with various programs

### Phase 3: Enhanced Features

1. **Nesting depth visualization** ✅ - Loop brackets colored by depth (6-color cycle)
   - Depth 0: Magenta
   - Depth 1: Cyan
   - Depth 2: Orange
   - Depth 3: Pink
   - Depth 4: Lime
   - Depth 5: Light blue
   - Cycles back to magenta for deeper nesting
2. HTML output format (future)
3. Custom themes expansion (future)
4. Context-aware display (future)
5. Integration with TUI debugger (future)

## Benefits

### For Users
- **Better code review**: Easier to spot patterns and errors
- **Learning**: Visual feedback helps understand BF structure
- **Documentation**: Generate highlighted code snippets
- **Debugging**: Quickly identify problematic sections

### For Developers
- **Reusable**: One highlighter for all tools
- **Testable**: Clear separation of highlighting logic
- **Extensible**: Easy to add new output formats
- **Maintainable**: Single source of truth for syntax

### For Future Tools
- **TUI Debugger**: Use highlighter for code display
- **REPL**: Syntax-highlighted input
- **IDE Plugins**: Export highlighting logic
- **Web Tools**: Generate HTML for online interpreters

## Testing Strategy

### Unit Tests (Library)
```rust
#[test]
fn test_highlight_arithmetic() {
    let highlighter = SyntaxHighlighter::new();
    let code = highlighter.highlight("+++---");
    assert_eq!(code.spans[0].style, SpanStyle::Arithmetic);
}

#[test]
fn test_highlight_nested_loops() {
    let highlighter = SyntaxHighlighter::new();
    let code = highlighter.highlight("[[]]");
    assert_eq!(code.spans[0].style, SpanStyle::LoopStart(0));
    assert_eq!(code.spans[1].style, SpanStyle::LoopStart(1));
}

#[test]
fn test_ansi_output() {
    let highlighter = SyntaxHighlighter::new();
    let code = highlighter.highlight("+-.,");
    let ansi = code.to_ansi();
    assert!(ansi.contains("\x1b[")); // Contains ANSI codes
}
```

### Integration Tests (Tool)
```bash
# Test basic highlighting
ferrous-cortex-tool view programs/basic/hello_world.bf

# Test with line numbers
ferrous-cortex-tool view programs/basic/simple.bf --line-numbers

# Test plain output
ferrous-cortex-tool view programs/advanced/quine.bf --plain > output.txt
```

## Dependencies

**New dependency for ANSI colors**:
```toml
[dependencies]
termcolor = "1.4"  # Cross-platform terminal colors
# OR
ansi_term = "0.12"  # Simple ANSI color codes
```

**Recommendation**: Use `termcolor` for better Windows support.

## Alternative Names

Instead of `view`, consider:
- `highlight` - More descriptive
- `show` - Simple and clear
- `cat` - Like Unix cat with colors
- `print` - Obvious purpose
- `format` - But this might conflict with code formatting

**Recommendation**: `view` is good - clear and concise.

## Future Extensions

### Smart Highlighting
- Detect common patterns (cell clear `[-]`, cell move `[>]`)
- Highlight inefficient patterns (from validator)
- Show optimizable sequences

### Interactive Mode
- Highlight as you type (for REPL)
- Show AST structure on hover
- Link to documentation

### Integration Points
- VS Code extension
- GitHub syntax highlighting
- Online playground
- Documentation generator

## Success Criteria

- ✅ Syntax module in library compiles and passes tests (10 tests passing)
- ✅ View command works with all example programs
- ✅ Colors display correctly on common terminals
- ✅ Plain output option works for piping
- ✅ Line numbers align properly
- ✅ Comments are visually distinct
- ✅ Loop nesting is clear with depth-based coloring
- ✅ Nesting depth tracking works across lines and handles unmatched brackets
- ✅ Performance is acceptable (< 100ms for 10KB file)

## Example Output Showcase

### Simple Program
```
$ ferrous-cortex-tool view programs/basic/simple.bf

1 │ * Output the letter 'H'
2 │ ++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.
```

### Complex Program
```
$ ferrous-cortex-tool view programs/advanced/fibonacci.bf --line-numbers --show-nesting

  1 │ * Fibonacci sequence generator
  2 │ +++++++++++
  3 │ >+>>>>++++++++++++++++++++++++++++++++++++++++++++
  4 │ >++++++++++++++++++++++++++++++++<<<<<<[>[>>>>>>+>
  5 │ +<<<<<<<-]>>>>>>>[<<<<<<<+>>>>>>>-]<[>++++++++++[-
  6 │ <-[>>+>+<<<-]>>>[<<<+>>>-]+<[>[-]<[-]]>[<<[>>>+<<<
  7 │   └─ Loop depth 1
  8 │ -]>>[-]]<<]>>>[>>+>+<<<-]>>>[<<<+>>>-]+<[>[-]<[-]]
  9 │ >[<<+>>[-]]<<<<<<<]
 10 │   └─ Loop depth 1 end
```

## Related Work

- Syntax highlighting in other interpreters
- BF syntax in VS Code, Vim, Emacs
- Pygments BF lexer
- GitHub Linguist BF grammar

## Conclusion

A reusable syntax highlighter:
1. Improves UX across all tools
2. Makes BF code more readable
3. Helps with debugging and learning
4. Positions us for future IDE integrations

Start with basic ANSI colors, then expand to HTML and custom themes as needed.
