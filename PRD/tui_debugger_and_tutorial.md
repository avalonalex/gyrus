# PRD: TUI Debugger and Interactive Tutorial

## Executive Summary

Build a **visual terminal debugger** with full step-through execution, breakpoints, and memory inspection, leveraging FerrousCortex's existing hook infrastructure. Extend it with an **interactive tutorial mode** inspired by "The Little Schemer" to teach BrainFuck concepts and demonstrate Turing completeness.

**Key Goals:**
1. **Visual debugging:** See code, memory, and execution state simultaneously
2. **Interactive control:** Breakpoints, step-through, watch expressions
3. **Educational mode:** Guided lessons teaching BF concepts
4. **Professional UX:** Intuitive key bindings, responsive interface

## Current Infrastructure (Already Available!)

### Hook System (Perfect for Debugging) ✅

We already have the complete hook infrastructure:

```rust
pub trait ExecutionHook {
    fn before_instruction(&mut self, ctx: &HookContext) -> HookDecision;
    fn after_instruction(&mut self, ctx: &HookContext) -> HookDecision;
    fn on_loop_enter(&mut self, ctx: &HookContext) -> HookDecision;
    fn on_loop_exit(&mut self, ctx: &HookContext) -> HookDecision;
    fn on_completion(&mut self, ctx: &HookContext);
}

pub struct HookContext<'a> {
    pub memory: &'a [u8],           // Current memory state
    pub pointer: MemoryAddress,      // Pointer position
    pub step_count: StepCount,       // Steps executed
    pub source_location: Option<SourceLocation>,  // Line/column
    pub loop_depth: usize,           // Nesting depth
}

pub enum HookDecision {
    Continue,  // Keep executing
    Break,     // Pause execution
    Skip,      // Skip this instruction
}
```

**Debugger Integration:**
- ✅ Breakpoints → Check in `before_instruction`, return `Break`
- ✅ Step-through → Always return `Break`, resume on user input
- ✅ Memory inspection → Access via `ctx.memory`
- ✅ Source tracking → Use `ctx.source_location`

### Debug Symbols ✅

```rust
pub struct DebugInfo {
    // Maps instruction index → source location
    pub fn lookup(&self, instruction: usize) -> Option<SourceLocation>
}

pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}
```

**Debugger Integration:**
- ✅ Highlight current instruction in source view
- ✅ Map breakpoints from line:column to instruction index
- ✅ Show execution location in real-time

### Execution Stats ✅

```rust
pub struct ExecutionStats {
    pub total_steps: StepCount,
    pub loop_iterations: u64,
    pub peak_memory_used: MemoryAddress,
    pub cells_modified: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}
```

**Debugger Integration:**
- ✅ Display stats in status panel
- ✅ Show performance metrics
- ✅ Track I/O operations

## TUI Technology Stack

### Recommended: ratatui + crossterm

**ratatui** (formerly tui-rs) - Terminal UI framework
- Widget-based layout system
- Rich components (lists, tables, charts, gauges)
- Efficient rendering (minimal redraws)
- Active community, well-maintained

**crossterm** - Terminal manipulation
- Cross-platform (Windows, macOS, Linux)
- Event handling (keyboard, mouse)
- Color support (ANSI, RGB)
- Already used by ratatui

**Dependencies:**
```toml
[dependencies]
ratatui = "0.26"
crossterm = "0.27"
```

## Debugger UI Design

### Layout Overview

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ FerrousCortex Debugger │ hanoi.bf │ Running │ Step: 1234/∞ │ [F1] Help      │
├────────────────────────────┬─────────────────────────────────────────────────┤
│ Source Code (60%)          │ Memory View (40%)                               │
│                            │                                                 │
│   1 │ +++                  │ Pointer: 5 │ Cell Value: 42                     │
│   2 │ [                    │ ┌─────────────────────────────────────────────┐ │
│ ► 3 │   >                  │ │ Addr │ 0  1  2  3  4  5  6  7  8  9  A  B  │ │
│   4 │   +                  │ ├──────┼─────────────────────────────────────┤ │
│   5 │   <                  │ │  000 │ 00 00 00 00 00[2A]00 00 00 00 00 00 │ │
│   6 │   -                  │ │  010 │ 00 00 00 00 00 00 00 00 00 00 00 00 │ │
│   7 │ ]                    │ │  020 │ 00 00 00 00 00 00 00 00 00 00 00 00 │ │
│   8 │ .                    │ └─────────────────────────────────────────────┘ │
│                            │                                                 │
│ Breakpoints: 2, 7          │ Watch: cell[0], cell[5]                         │
│                            │ cell[0] = 3                                     │
│                            │ cell[5] = 42 (changed)                          │
├────────────────────────────┴─────────────────────────────────────────────────┤
│ Output                                                                        │
│ Hello World!                                                                  │
├───────────────────────────────────────────────────────────────────────────────┤
│ Status: Paused at line 3:4 │ Loop depth: 1 │ Modified: 2 cells               │
│ [Space] Step │ [Enter] Continue │ [B] Breakpoint │ [Q] Quit                  │
└───────────────────────────────────────────────────────────────────────────────┘
```

### Panel Breakdown

#### 1. Header Bar (1 line)
```
┌──────────────────────────────────────────────────────────────────────────────┐
│ FerrousCortex Debugger │ hanoi.bf │ Running │ Step: 1234/∞ │ [F1] Help      │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Contents:**
- Application name
- Current file
- Execution state (Running, Paused, Breakpoint, Error)
- Step counter
- Help shortcut

#### 2. Source Code Panel (60% width, left)
```
┌────────────────────────────┐
│ Source Code                │
│                            │
│   1 │ +++                  │
│   2 │ [                    │ ← Breakpoint indicator (red dot)
│ ► 3 │   >                  │ ← Current line (highlighted, green arrow)
│   4 │   +                  │
│   5 │   <                  │
│   6 │   -                  │
│   7 │ ]                    │ ← Breakpoint indicator
│   8 │ .                    │
│                            │
│ Breakpoints: 2, 7          │ ← Breakpoint list
└────────────────────────────┘
```

**Features:**
- Line numbers
- Current instruction highlighted (bold green + arrow)
- Breakpoint indicators (● red dot)
- Syntax highlighting (same colors as our profiler):
  - Pointer ops: cyan (`>`, `<`)
  - Cell ops: green (`+`, `-`)
  - Loops: orange (`[`, `]`)
  - I/O: yellow (`.`, `,`)
- Scroll to follow execution
- Breakpoint list at bottom

**Implementation:**
```rust
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::style::{Color, Modifier, Style};

fn render_source_panel(
    frame: &mut Frame,
    area: Rect,
    source: &str,
    current_line: usize,
    breakpoints: &HashSet<usize>,
) {
    let items: Vec<ListItem> = source
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + 1;
            let has_breakpoint = breakpoints.contains(&line_num);
            let is_current = line_num == current_line;

            let prefix = match (is_current, has_breakpoint) {
                (true, true) => "►●",
                (true, false) => "► ",
                (false, true) => " ●",
                (false, false) => "  ",
            };

            let style = if is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = format!("{:3} │ {}{}", line_num, prefix, line);
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Source Code"));

    frame.render_widget(list, area);
}
```

#### 3. Memory View Panel (40% width, right)
```
┌─────────────────────────────────────────────┐
│ Memory View                                 │
│                                             │
│ Pointer: 5 │ Cell Value: 42 (0x2A)         │ ← Current cell
│ ┌─────────────────────────────────────────┐ │
│ │ Addr │ 0  1  2  3  4  5  6  7  8  9  A  │ │ ← Hex view
│ ├──────┼─────────────────────────────────┤ │
│ │  000 │ 00 00 00 00 00[2A]00 00 00 00 00│ │ ← Current cell highlighted
│ │  010 │ 00 00 00 00 00 00 00 00 00 00 00│ │
│ │  020 │ 00 00 00 00 00 00 00 00 00 00 00│ │
│ │  030 │ 00 00 00 00 00 00 00 00 00 00 00│ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ ASCII View:                                 │ ← ASCII representation
│ .....*......                                │
│                                             │
│ Watch Expressions:                          │ ← Watched cells
│ • cell[0] = 3                               │
│ • cell[5] = 42 (changed this step)          │ ← Highlight changes
│ • sum(0..10) = 55                           │
└─────────────────────────────────────────────┘
```

**Features:**
- Current pointer position and value
- Hex dump (12 bytes per row for readability)
- Current cell highlighted with brackets `[2A]`
- ASCII view showing printable characters
- Watch expressions with change tracking
- Scroll to follow pointer or show arbitrary range

**Smart Display:**
- Auto-scroll to keep pointer in view
- Highlight modified cells (yellow background)
- Show only non-zero regions (collapsible)
- Color coding:
  - Current cell: green brackets
  - Modified cells: yellow background
  - Watched cells: cyan text

**Implementation:**
```rust
fn render_memory_panel(
    frame: &mut Frame,
    area: Rect,
    memory: &[u8],
    pointer: usize,
    modified: &HashSet<usize>,
    watches: &[(String, usize)],
) {
    // Split into sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Current cell info
            Constraint::Min(10),    // Hex view
            Constraint::Length(3),  // ASCII view
            Constraint::Length(5),  // Watch expressions
        ])
        .split(area);

    // Current cell info
    let current_value = memory[pointer];
    let info = format!(
        "Pointer: {} │ Cell Value: {} (0x{:02X})",
        pointer, current_value, current_value
    );
    let info_widget = Paragraph::new(info)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(info_widget, chunks[0]);

    // Hex view (scrollable around pointer)
    let start = pointer.saturating_sub(24);
    let end = (start + 96).min(memory.len());

    let hex_lines: Vec<String> = memory[start..end]
        .chunks(12)
        .enumerate()
        .map(|(i, chunk)| {
            let addr = start + i * 12;
            let hex: Vec<String> = chunk
                .iter()
                .enumerate()
                .map(|(j, &byte)| {
                    let is_current = (addr + j) == pointer;
                    let is_modified = modified.contains(&(addr + j));

                    if is_current {
                        format!("[{:02X}]", byte)  // Highlighted
                    } else if is_modified {
                        format!("*{:02X}*", byte)  // Modified
                    } else {
                        format!(" {:02X} ", byte)
                    }
                })
                .collect();

            format!(" {:03X} │ {}", addr, hex.join(""))
        })
        .collect();

    let hex_widget = Paragraph::new(hex_lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Memory (Hex)"));
    frame.render_widget(hex_widget, chunks[1]);

    // ASCII view
    let ascii: String = memory[start..end]
        .iter()
        .map(|&b| {
            if b >= 32 && b < 127 {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    let ascii_widget = Paragraph::new(ascii)
        .block(Block::default().borders(Borders::ALL).title("ASCII"));
    frame.render_widget(ascii_widget, chunks[2]);

    // Watch expressions
    let watch_items: Vec<ListItem> = watches
        .iter()
        .map(|(name, addr)| {
            let value = memory[*addr];
            let changed = modified.contains(addr);
            let style = if changed {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let text = if changed {
                format!("• {} = {} (changed)", name, value)
            } else {
                format!("• {} = {}", name, value)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let watch_widget = List::new(watch_items)
        .block(Block::default().borders(Borders::ALL).title("Watch"));
    frame.render_widget(watch_widget, chunks[3]);
}
```

#### 4. Output Panel (bottom, 20% height)
```
┌───────────────────────────────────────────────────────────────────────────────┐
│ Output                                                                        │
│ Hello World!                                                                  │
│ [5 bytes written]                                                             │
└───────────────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Program output (stdout)
- Input echo (stdin)
- Scrollable history
- Byte count

#### 5. Status Bar (bottom, 2 lines)
```
┌───────────────────────────────────────────────────────────────────────────────┐
│ Status: Paused at line 3:4 │ Loop depth: 1 │ Modified: 2 cells               │
│ [Space] Step │ [Enter] Continue │ [B] Breakpoint │ [Q] Quit                  │
└───────────────────────────────────────────────────────────────────────────────┘
```

**Line 1:** Status information
- Execution state and location
- Loop depth
- Statistics (modified cells, I/O count)

**Line 2:** Key bindings (context-sensitive)
- Show available commands based on state
- Highlight most common actions

## Key Bindings

### Primary Controls (Always Available)

| Key | Action | Description |
|-----|--------|-------------|
| `Space` | **Step** | Execute one instruction, then pause |
| `Enter` / `C` | **Continue** | Run until next breakpoint or end |
| `Q` / `Ctrl+C` | **Quit** | Exit debugger |
| `R` | **Restart** | Reset to beginning |
| `F1` / `?` | **Help** | Show key bindings overlay |

### Breakpoint Controls

| Key | Action | Description |
|-----|--------|-------------|
| `B` | **Toggle Breakpoint** | Add/remove breakpoint at cursor line |
| `Shift+B` | **List Breakpoints** | Show all breakpoints |
| `Ctrl+B` | **Clear All Breakpoints** | Remove all breakpoints |
| `N` | **Next Breakpoint** | Continue to next breakpoint |

### Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `↑` / `K` | **Up** | Scroll source code up |
| `↓` / `J` | **Down** | Scroll source code down |
| `G` | **Go to Line** | Jump to specific line |
| `Home` | **Top** | Scroll to beginning |
| `End` | **Bottom** | Scroll to end |

### Memory View

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | **Switch Panel** | Toggle focus source ↔ memory |
| `M` | **Memory Mode** | Cycle display modes (hex/decimal/ASCII) |
| `W` | **Add Watch** | Add cell to watch list |
| `Shift+W` | **Remove Watch** | Remove watch expression |
| `PgUp` / `PgDn` | **Scroll Memory** | Navigate memory view |
| `F` | **Follow Pointer** | Auto-scroll memory to pointer |

### Execution Control

| Key | Action | Description |
|-----|--------|-------------|
| `S` | **Step Over** | Execute until loop completes |
| `O` | **Step Out** | Exit current loop |
| `I` | **Step Into** | Enter loop (default for `Space`) |
| `Ctrl+R` | **Run to Cursor** | Execute until cursor line |

### Advanced

| Key | Action | Description |
|-----|--------|-------------|
| `E` | **Evaluate** | Evaluate expression on current state |
| `H` | **History** | Show execution history (time-travel) |
| `Ctrl+S` | **Snapshot** | Save current state |
| `Ctrl+L` | **Load Snapshot** | Restore saved state |
| `T` | **Toggle Trace** | Enable/disable execution trace |

### Vim-style Alternatives (Optional)

For users familiar with Vim:

| Vim | Standard | Action |
|-----|----------|--------|
| `h/j/k/l` | Arrow keys | Navigate |
| `:b` | `B` | Toggle breakpoint |
| `:c` | `Enter` | Continue |
| `:s` | `Space` | Step |
| `:q` | `Q` | Quit |

## Debugger Hook Implementation

### DebuggerHook Structure

```rust
use ferrous_cortex::hooks::{ExecutionHook, HookContext, HookDecision};
use std::collections::HashSet;

pub struct DebuggerHook {
    // State
    state: DebuggerState,

    // Breakpoints
    breakpoints: HashSet<usize>,  // Set of instruction indices

    // Execution control
    step_mode: bool,              // Pause after every instruction
    run_to_cursor: Option<usize>, // Run until specific instruction

    // History (time-travel debugging)
    snapshots: Vec<MemorySnapshot>,
    max_snapshots: usize,

    // Watch expressions
    watches: Vec<WatchExpression>,

    // Change tracking
    modified_cells: HashSet<usize>,
    last_memory: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebuggerState {
    Running,        // Executing normally
    Paused,         // User paused
    Breakpoint,     // Hit breakpoint
    StepMode,       // Step-by-step
    Error(String),  // Runtime error
    Completed,      // Program finished
}

pub struct MemorySnapshot {
    step: u64,
    memory: Vec<u8>,
    pointer: usize,
    source_location: Option<SourceLocation>,
}

pub struct WatchExpression {
    name: String,
    cell_index: usize,
    last_value: u8,
}

impl ExecutionHook for DebuggerHook {
    fn before_instruction(&mut self, ctx: &HookContext) -> HookDecision {
        // Track modified cells
        self.track_changes(ctx.memory);

        // Check if we should pause
        if self.should_pause(ctx) {
            self.state = DebuggerState::Paused;

            // Render UI and wait for user input
            self.render_ui(ctx);
            self.wait_for_command(ctx);

            // User may have changed state (continue, step, quit)
            match self.state {
                DebuggerState::Completed => HookDecision::Break,
                _ => HookDecision::Continue,
            }
        } else {
            HookDecision::Continue
        }
    }

    fn after_instruction(&mut self, ctx: &HookContext) -> HookDecision {
        // Take snapshot for time-travel debugging
        if self.snapshots.len() < self.max_snapshots {
            self.take_snapshot(ctx);
        }

        HookDecision::Continue
    }

    fn on_loop_enter(&mut self, ctx: &HookContext) -> HookDecision {
        // Useful for "step over" - skip entire loop
        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, ctx: &HookContext) -> HookDecision {
        HookDecision::Continue
    }

    fn on_completion(&mut self, ctx: &HookContext) {
        self.state = DebuggerState::Completed;
        self.render_ui(ctx);
    }
}

impl DebuggerHook {
    fn should_pause(&self, ctx: &HookContext) -> bool {
        // Pause if:
        // 1. Step mode enabled
        if self.step_mode {
            return true;
        }

        // 2. Breakpoint at current instruction
        if let Some(loc) = ctx.source_location {
            if self.breakpoints.contains(&ctx.step_count.get()) {
                return true;
            }
        }

        // 3. Run to cursor reached
        if let Some(target) = self.run_to_cursor {
            if ctx.step_count.get() >= target {
                self.run_to_cursor = None;
                return true;
            }
        }

        false
    }

    fn track_changes(&mut self, current_memory: &[u8]) {
        self.modified_cells.clear();

        for (i, (&old, &new)) in self.last_memory.iter().zip(current_memory).enumerate() {
            if old != new {
                self.modified_cells.insert(i);
            }
        }

        // Update last memory snapshot
        self.last_memory.copy_from_slice(current_memory);
    }

    fn wait_for_command(&mut self, ctx: &HookContext) {
        use crossterm::event::{self, Event, KeyCode, KeyModifiers};

        loop {
            if let Ok(Event::Key(key)) = event::read() {
                match (key.code, key.modifiers) {
                    // Step
                    (KeyCode::Char(' '), _) => {
                        self.step_mode = true;
                        self.state = DebuggerState::StepMode;
                        break;
                    }

                    // Continue
                    (KeyCode::Enter, _) | (KeyCode::Char('c'), _) => {
                        self.step_mode = false;
                        self.state = DebuggerState::Running;
                        break;
                    }

                    // Toggle breakpoint
                    (KeyCode::Char('b'), _) => {
                        if let Some(loc) = ctx.source_location {
                            let inst_idx = ctx.step_count.get();
                            if self.breakpoints.contains(&inst_idx) {
                                self.breakpoints.remove(&inst_idx);
                            } else {
                                self.breakpoints.insert(inst_idx);
                            }
                            self.render_ui(ctx);
                        }
                    }

                    // Quit
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        self.state = DebuggerState::Completed;
                        break;
                    }

                    // Restart
                    (KeyCode::Char('r'), _) => {
                        // TODO: Signal restart
                        break;
                    }

                    _ => {}
                }
            }
        }
    }
}
```

## Interactive Tutorial Mode

### Inspired by "The Little Schemer"

The tutorial follows Socratic dialogue style:
- Ask questions
- Let user experiment
- Reveal insights through discovery
- Build complexity gradually

### Tutorial Structure

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ FerrousCortex Tutorial │ Lesson 3: Loops │ Progress: 3/12                    │
├────────────────────────────┬─────────────────────────────────────────────────┤
│ Instruction                │ Your Code                                       │
│                            │                                                 │
│ Question:                  │   1 │                                           │
│ What happens when you run  │   2 │                                           │
│ a loop with a non-zero     │   3 │                                           │
│ cell?                      │   4 │                                           │
│                            │   5 │                                           │
│ Try this:                  │   6 │                                           │
│   ++[>+<-]                 │                                                 │
│                            │ [Load Example] [Run] [Step]                     │
│ This copies the value from │                                                 │
│ cell 0 to cell 1.          │                                                 │
│                            │                                                 │
│ Can you explain why?       ├─────────────────────────────────────────────────┤
│                            │ Memory                                          │
│ Hint: Watch what happens   │                                                 │
│ to both cells at each      │ cell[0]: 2 → 0                                  │
│ step.                      │ cell[1]: 0 → 2                                  │
├────────────────────────────┴─────────────────────────────────────────────────┤
│ Output: (empty)                                                               │
├───────────────────────────────────────────────────────────────────────────────┤
│ [Space] Step │ [Enter] Run │ [N] Next Lesson │ [B] Back                      │
└───────────────────────────────────────────────────────────────────────────────┘
```

### Lesson Outline

#### Lesson 0: Welcome
```
Welcome to FerrousCortex!

BrainFuck is one of the simplest programming languages,
but it's Turing complete - it can compute anything!

You have:
  • 30,000 memory cells (like an array)
  • A pointer (like an index)
  • 8 commands

Let's learn them!

[Press Enter to continue]
```

#### Lesson 1: Incrementing
```
The + command increments the current cell.

Try it: +

What's the value of cell[0]?

[Step through and watch]

Answer: 1

Now try: +++

What's the value now?

Answer: 3

You've learned: + adds 1 to the current cell.
```

#### Lesson 2: The Pointer
```
The > command moves the pointer right.
The < command moves the pointer left.

Try this: +>++>+++

Question: What are the values?
  cell[0] = ?
  cell[1] = ?
  cell[2] = ?

[Step through to find out]

Answer:
  cell[0] = 1
  cell[1] = 2
  cell[2] = 3

You've learned: > and < move between cells.
```

#### Lesson 3: Loops
```
Loops run while the current cell is non-zero.
[ starts a loop
] ends a loop

Try this: ++[>+<-]

Question: What does this do?

[Step through carefully]

Answer:
  • Starts with cell[0] = 2
  • Loop runs twice (while cell[0] != 0)
  • Each iteration: cell[1]++, cell[0]--
  • Result: Moves value from cell[0] to cell[1]

This is a MOVE operation!

You've learned: Loops enable complex operations.
```

#### Lesson 4: Clearing
```
How would you set a cell to zero?

Try: [-]

Question: What happens?

Answer:
  • Loop runs while cell != 0
  • Each iteration: cell--
  • Stops when cell = 0

This is the idiomatic way to clear a cell!

Challenge: Can you clear cell[5]?
Hint: You need to move the pointer first!

[Try: >>>>>[-]]
```

#### Lesson 5: Multiplication
```
Can you multiply two numbers?

Example: 3 × 4 = 12

Try this: +++[>++++<-]

Question: What does this compute?

[Step through]

Answer:
  • cell[0] = 3 (loop counter)
  • Each iteration: add 4 to cell[1]
  • Result: cell[1] = 3 × 4 = 12

You've learned: Loops can implement multiplication!
```

#### Lesson 6: Turing Completeness
```
Congratulations! You now understand why BrainFuck
is Turing complete.

A language is Turing complete if it has:
  1. Arbitrary memory ✓ (30,000 cells, unbounded mode)
  2. Conditional branching ✓ (loops with [ ])
  3. Basic arithmetic ✓ (+, -)

With these, you can compute ANYTHING:
  • Add, subtract, multiply, divide
  • Compare numbers
  • Implement variables, arrays, functions
  • Build a universal Turing machine!

[Next: Advanced Patterns]
```

#### Lesson 7-12: Advanced Topics
- Lesson 7: Input/Output (`,` and `.`)
- Lesson 8: Nested Loops
- Lesson 9: Conditional Execution (simulating if/else)
- Lesson 10: Subroutines (function-like patterns)
- Lesson 11: Data Structures (arrays, stacks)
- Lesson 12: The Halting Problem (why BF can't solve it)

### Tutorial Implementation

```rust
pub struct Tutorial {
    lessons: Vec<Lesson>,
    current_lesson: usize,
    user_code: String,
    debugger: DebuggerHook,
}

pub struct Lesson {
    title: String,
    instruction: String,
    example_code: Option<String>,
    hints: Vec<String>,
    expected_result: Option<ExpectedResult>,
    next_lesson_unlock: Box<dyn Fn(&ExecutionStats) -> bool>,
}

pub enum ExpectedResult {
    MemoryState(Vec<(usize, u8)>),  // cell[index] = value
    Output(String),
    Custom(Box<dyn Fn(&ExecutionStats) -> bool>),
}

impl Tutorial {
    pub fn new() -> Self {
        Self {
            lessons: create_lessons(),
            current_lesson: 0,
            user_code: String::new(),
            debugger: DebuggerHook::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            let lesson = &self.lessons[self.current_lesson];

            // Render tutorial UI
            self.render_lesson(lesson)?;

            // Wait for user action
            match self.handle_input()? {
                TutorialAction::LoadExample => {
                    if let Some(code) = &lesson.example_code {
                        self.user_code = code.clone();
                    }
                }
                TutorialAction::Run => {
                    self.run_user_code()?;
                }
                TutorialAction::Step => {
                    self.debugger.step_mode = true;
                    self.run_user_code()?;
                }
                TutorialAction::NextLesson => {
                    if self.can_advance() {
                        self.current_lesson += 1;
                    } else {
                        self.show_hint()?;
                    }
                }
                TutorialAction::PreviousLesson => {
                    if self.current_lesson > 0 {
                        self.current_lesson -= 1;
                    }
                }
                TutorialAction::Quit => break,
            }
        }

        Ok(())
    }

    fn can_advance(&self) -> bool {
        let lesson = &self.lessons[self.current_lesson];
        // Check if user has completed the lesson
        // (e.g., achieved expected result)
        true  // TODO: Implement lesson completion check
    }
}
```


## Crate Architecture

### Separation of Concerns

```
crates/
├── ferrous-cortex/              # Core library (existing)
├── ferrous-cortex-cli/          # Interpreter CLI (existing)
├── ferrous-cortex-tool/         # Dev tools (existing)
├── ferrous-cortex-codegen/      # Cranelift IR translation (future)
├── ferrous-cortex-jit/          # JIT compiler (future)
├── ferrous-cortex-tui/          # NEW: Shared TUI components
│   ├── src/
│   │   ├── lib.rs
│   │   ├── panels/
│   │   │   ├── source.rs        # Source code panel
│   │   │   ├── memory.rs        # Memory view panel
│   │   │   ├── output.rs        # Output panel
│   │   │   └── status.rs        # Status bar
│   │   ├── widgets/
│   │   │   ├── hex_dump.rs      # Hex memory dump widget
│   │   │   ├── watch.rs         # Watch expression widget
│   │   │   └── help.rs          # Help overlay
│   │   ├── theme.rs             # Color scheme (BF syntax)
│   │   └── layout.rs            # Common layouts
│   └── Cargo.toml
├── ferrous-cortex-debug/        # NEW: Debugger binary
│   ├── src/
│   │   ├── main.rs
│   │   ├── debugger.rs          # Debugger state machine
│   │   ├── hook.rs              # DebuggerHook implementation
│   │   ├── breakpoint.rs        # Breakpoint management
│   │   └── ui.rs                # Debugger UI (uses tui crate)
│   └── Cargo.toml
└── ferrous-cortex-tutorial/     # NEW: Tutorial binary (separate!)
    ├── src/
    │   ├── main.rs
    │   ├── tutorial.rs          # Tutorial state machine
    │   ├── lessons/
    │   │   ├── mod.rs
    │   │   ├── lesson_01.rs     # Incrementing
    │   │   ├── lesson_02.rs     # Pointer movement
    │   │   └── ...              # Lessons 3-12
    │   └── ui.rs                # Tutorial UI (uses tui crate)
    └── Cargo.toml
```

### Why Separate Crates?

**ferrous-cortex-tui (shared library):**
- ✅ Reusable TUI components
- ✅ Common BF syntax highlighting theme
- ✅ Memory view, source view, output panels
- ✅ Both debugger and tutorial use these

**ferrous-cortex-debug (debugger binary):**
- 🎯 **Focus:** Professional debugging for developers
- 📦 **Binary size:** ~5MB (no tutorial lessons)
- 👥 **Audience:** BF developers, serious users
- ⚙️ **Features:** Breakpoints, watch, step-through

**ferrous-cortex-tutorial (tutorial binary):**
- 🎯 **Focus:** Teaching BF concepts to beginners
- 📦 **Binary size:** ~3MB (no debugging complexity)
- 👥 **Audience:** Learners, students, educators
- ⚙️ **Features:** Guided lessons, hints, exercises

**Benefits:**
- Users can install only what they need
- Each crate evolves independently
- Smaller binaries (no bundled unused features)
- Clear separation of concerns
- Shared components reduce duplication

## Implementation Plan

### Phase 0: Shared TUI Components (3 days)

**Goal:** Build reusable TUI library

- [ ] Create `ferrous-cortex-tui` crate
  - [ ] Add ratatui + crossterm dependencies
  - [ ] Set up library structure
- [ ] Implement panels:
  - [ ] `SourcePanel` - Display BF source with highlighting
  - [ ] `MemoryPanel` - Hex dump, ASCII view, watches
  - [ ] `OutputPanel` - Program output scrollable view
  - [ ] `StatusBar` - Status line with key hints
- [ ] Implement widgets:
  - [ ] `HexDump` - Memory hex dump with highlighting
  - [ ] `WatchList` - Watch expressions with change tracking
  - [ ] `HelpOverlay` - Keyboard shortcuts popup
- [ ] BF syntax theme:
  - [ ] Color scheme (cyan/green/orange/yellow)
  - [ ] Highlighting helpers
  - [ ] Current line indicator
  - [ ] Breakpoint indicator
- [ ] Common layouts:
  - [ ] Two-column layout (source + memory)
  - [ ] Three-panel layout (source + memory + output)

**Testing:** Create example program using all panels

### Phase 1: Basic Debugger (1 week)

**Goal:** Step-through execution with source and memory view

- [ ] Create `ferrous-cortex-debug` crate
  - [ ] Depend on `ferrous-cortex-tui`
  - [ ] Set up debugger binary
- [ ] Implement `DebuggerHook`
  - [ ] Integration with hook system
  - [ ] Step mode (pause after each instruction)
  - [ ] Continue mode (run to end)
- [ ] Source view panel
  - [ ] Syntax highlighting
  - [ ] Current line indicator
  - [ ] Line numbers
- [ ] Memory view panel
  - [ ] Hex dump
  - [ ] Current pointer highlight
  - [ ] ASCII view
- [ ] Key bindings
  - [ ] Space: step
  - [ ] Enter: continue
  - [ ] Q: quit
- [ ] Status bar
  - [ ] Execution state
  - [ ] Step counter
  - [ ] Key hints

**Testing:** Run simple.bf, verify stepping works

### Phase 2: Breakpoints & Watch (1 week)

**Goal:** Full debugging features

- [ ] Breakpoint support
  - [ ] Toggle with `B` key
  - [ ] Visual indicators in source
  - [ ] Pause execution at breakpoints
  - [ ] List all breakpoints
  - [ ] Clear all breakpoints
- [ ] Watch expressions
  - [ ] Add watch with `W` key
  - [ ] Show watched cell values
  - [ ] Highlight changes
  - [ ] Remove watch
- [ ] Enhanced memory view
  - [ ] Modified cell highlighting
  - [ ] Follow pointer mode
  - [ ] Scroll to arbitrary address
  - [ ] Display modes (hex/decimal/ASCII)
- [ ] Advanced key bindings
  - [ ] Run to cursor
  - [ ] Step over (skip loops)
  - [ ] Restart execution

**Testing:** Debug hanoi.bf with breakpoints

### Phase 3: Tutorial Binary (1 week)

**Goal:** Interactive lessons teaching BF

- [ ] Create `ferrous-cortex-tutorial` crate
  - [ ] Depend on `ferrous-cortex-tui`
  - [ ] Set up tutorial binary
- [ ] Lesson framework
  - [ ] Lesson data structure
  - [ ] Lesson progression logic
  - [ ] Completion checking
- [ ] Tutorial UI (using shared TUI components)
  - [ ] Instruction panel (custom)
  - [ ] Code editor panel (reuse SourcePanel)
  - [ ] Memory view (reuse MemoryPanel)
  - [ ] Output panel (reuse OutputPanel)
  - [ ] Progress indicator
- [ ] Implement 12 lessons
  - [ ] Lessons 0-6: Basics (welcome, +, >, loops, clear, multiply, Turing)
  - [ ] Lessons 7-9: Intermediate (I/O, nested loops, conditionals)
  - [ ] Lessons 10-12: Advanced (subroutines, data structures, halting)
- [ ] Tutorial-specific features
  - [ ] Hints system
  - [ ] Load example code
  - [ ] Expected result checking
  - [ ] Success feedback
  - [ ] Step-through for learning (simpler than debugger)

**Testing:** Complete all 12 lessons, verify learning flow

### Phase 4: Advanced Features (Optional)

**Goal:** Time-travel debugging, optimization

- [ ] Time-travel debugging
  - [ ] Snapshot history
  - [ ] Step backward
  - [ ] Rewind to snapshot
  - [ ] Execution timeline visualization
- [ ] Performance
  - [ ] Optimize rendering (only redraw changed panels)
  - [ ] Handle large programs (>10K lines)
  - [ ] Lazy memory view (don't render all 30K cells)
- [ ] Export/Import
  - [ ] Save debugging session
  - [ ] Load saved session
  - [ ] Export execution trace
- [ ] Mouse support
  - [ ] Click to set breakpoint
  - [ ] Click to inspect cell
  - [ ] Drag to scroll

## Success Metrics

### Phase 1 (Basic Debugger)
✅ **Can step through simple.bf instruction by instruction**
✅ **Source view highlights current line correctly**
✅ **Memory view shows pointer and values**
✅ **Key bindings work reliably**

### Phase 2 (Full Debugger)
✅ **Can set/remove breakpoints with visual feedback**
✅ **Watch expressions track cell changes**
✅ **Can debug complex programs (hanoi.bf)**
✅ **All key bindings implemented**

### Phase 3 (Tutorial)
✅ **All 12 lessons implemented**
✅ **Tutorial UI is clear and intuitive**
✅ **New users can complete lessons 0-6 in 30 minutes**
✅ **Lessons successfully teach BF concepts**

### Phase 4 (Advanced - Optional)
✅ **Time-travel debugging works smoothly**
✅ **Can handle large programs efficiently**
✅ **Export/import preserves full session state**

## Dependencies

### ferrous-cortex-tui (Shared Library)

```toml
[package]
name = "ferrous-cortex-tui"
version = "0.3.0"
edition = "2024"

[dependencies]
ferrous-cortex = { path = "../ferrous-cortex" }
ratatui = "0.26"
crossterm = "0.27"
```

### ferrous-cortex-debug (Debugger Binary)

```toml
[package]
name = "ferrous-cortex-debug"
version = "0.3.0"
edition = "2024"

[[bin]]
name = "ferrous-cortex-debug"
path = "src/main.rs"

[dependencies]
ferrous-cortex = { path = "../ferrous-cortex" }
ferrous-cortex-tui = { path = "../ferrous-cortex-tui" }
anyhow = "1.0"
clap = { version = "4.5", features = ["derive"] }
```

### ferrous-cortex-tutorial (Tutorial Binary)

```toml
[package]
name = "ferrous-cortex-tutorial"
version = "0.3.0"
edition = "2024"

[[bin]]
name = "ferrous-cortex-tutorial"
path = "src/main.rs"

[dependencies]
ferrous-cortex = { path = "../ferrous-cortex" }
ferrous-cortex-tui = { path = "../ferrous-cortex-tui" }
anyhow = "1.0"
clap = { version = "4.5", features = ["derive"] }
```

## CLI Interface

### Debugger

```bash
# Basic debugger
ferrous-cortex-debug program.bf

# Debug with specific memory model
ferrous-cortex-debug program.bf --memory-model unbounded

# Debug with checked cells
ferrous-cortex-debug program.bf --cell-model checked

# Help
ferrous-cortex-debug --help
```

### Tutorial

```bash
# Start tutorial from beginning
ferrous-cortex-tutorial

# Start at specific lesson
ferrous-cortex-tutorial --lesson 5

# List all lessons
ferrous-cortex-tutorial --list

# Tutorial with custom memory size (for advanced lessons)
ferrous-cortex-tutorial --memory-size 100

# Help
ferrous-cortex-tutorial --help
```

## Documentation Needs

### User Documentation
- [ ] Tutorial mode guide (how to use lessons)
- [ ] Debugger key binding reference card
- [ ] Example debugging session walkthrough
- [ ] Tips and tricks

### Developer Documentation
- [ ] TUI architecture overview
- [ ] Hook integration guide
- [ ] Adding new lessons (tutorial extension)
- [ ] Custom watch expression syntax

## Conclusion

The TUI system with separate debugger and tutorial will:

✅ **Leverage existing infrastructure:** Hooks, debug symbols, execution stats
✅ **Shared components:** Reusable TUI library for consistency
✅ **Professional debugging:** Breakpoints, watch, step-through, time-travel
✅ **Effective teaching:** Interactive lessons, Socratic method, guided exercises
✅ **Demonstrate Turing completeness:** Build from basics to universal computation
✅ **Modular design:** Users install only what they need

**Three-Crate Architecture:**

1. **ferrous-cortex-tui** (library)
   - Shared panels, widgets, theme
   - Source view, memory view, output
   - Reusable across tools

2. **ferrous-cortex-debug** (binary)
   - Professional debugging tool
   - For BF developers
   - ~5MB binary

3. **ferrous-cortex-tutorial** (binary)
   - Interactive BF learning
   - For students and beginners
   - ~3MB binary

**Timeline:**
- Phase 0 (Shared TUI): 3 days
- Phase 1 (Basic Debugger): 1 week
- Phase 2 (Full Debugger): 1 week
- Phase 3 (Tutorial): 1 week
- **Total: ~3.5 weeks** to complete system

**Benefits:**
- Clear separation of concerns
- Smaller binaries (no bundled unused features)
- Independent evolution (debugger vs tutorial)
- Code reuse via shared library
- Professional UX with consistent design

This positions FerrousCortex as the **most comprehensive BrainFuck development environment** available, with both professional tools and educational resources! 🎯
