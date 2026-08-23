# Errors and Diagnostics

What gyrus reports when a program is wrong, and how to get source locations
out of it.

Back to the [README](../README.md).

## Error Handling

gyrus provides detailed error messages to help debug BrainFuck programs:

### Parse Errors

When your program has syntax errors, you'll see the exact location:

```
Error: Unmatched '[' at line 3, column 12
    1 | +++
    2 | [->+++<]
    3 | Some code [
      |            ^
    4 | More code
```

#### Multiple Bracket Errors

gyrus detects **all** bracket matching errors in a single pass, saving you time by showing all issues at once:

```
Found 3 bracket matching error(s):

Error 1:
Unmatched '[' at line 3, column 1
    1 | * Test file
    2 |
    3 | [>++
      | ^
    4 | [<--
    5 | [+++

Error 2:
Unmatched '[' at line 4, column 1
    3 | [>++
    4 | [<--
      | ^
    5 | [+++
    6 |

Error 3:
Unmatched '[' at line 5, column 1
    4 | [<--
    5 | [+++
      | ^
    6 |
```

This comprehensive error reporting helps you fix all bracket issues in one go instead of fixing them one at a time.

### Runtime Errors

Runtime errors include:
- **Memory out of bounds**: Attempting to access memory outside valid range
- **Cell overflow/underflow** (in checked mode): Cell arithmetic exceeds boundaries
- **Step limit exceeded**: Program exceeded maximum allowed instructions
- **Execution timeout**: Program took too long to execute
- **I/O errors**: Problems reading input or writing output

Runtime errors can include **syntax-highlighted source code** showing exactly where the error occurred when using the `--debug` flag:

```bash
# Debug mode: errors show source locations
gyrus program.bf --debug
```

```
Error: Cell overflow

At line 6, column 16:
   4 │ ++++++++++++++++++++++++++++++++++++++++++++++
   5 │ ++++++++++++++++++++++++++++++++++++++++++++++
   6 │ +++++++++++++++
       ^

Attempted to increment cell with value 255, but checked arithmetic prevents overflow.
```

**Syntax highlighting features (with `--debug`):**
- Commands color-coded by type (pointer movement in cyan, cell operations in green)
- Line numbers shown in gray
- Red caret (^) points at exact instruction that caused the error
- Comments rendered in gray
- Loop brackets color-coded by nesting depth

**Default mode (fast):**
By default, errors show detailed messages but without source locations. This is much faster, especially for large programs:

```
Error: Memory pointer out of bounds
Attempted to access cell 100, but memory size is 100 cells.
```

**When to use `--debug`:**
- 🐛 **Debugging issues**: Finding where in your code a problem occurs
- 📚 **Learning**: Understanding how BrainFuck programs execute
- 🔍 **Development**: Writing and testing new BrainFuck code

**When to skip `--debug` (default):**
- 🚀 **Production runs**: Running known-good programs quickly
- ⚡ **Large programs**: Mandelbrot, quines, and other complex programs (40x faster)
- 📊 **Benchmarking**: Measuring program performance

### Runtime Warnings

When using unbounded memory mode, the interpreter generates **warnings** when memory is expanded:

```
Runtime warning: Memory expanded from 30000 to 30001 bytes

At line 1, column 50001:
   1 │ >>>>>>>>>>>>>>>>...
                        ^
```

**Warning types:**
- **Memory expansion**: Memory grew dynamically (with unbounded memory model)

Warnings are shown with `--verbose` flag. Cell wrapping (255+1=0, 0-1=255) does not generate warnings as it's standard BrainFuck behavior.

### Preventing Infinite Loops

Use `--max-steps` or `--timeout` to prevent runaway programs:

```bash
# Prevent infinite loops with step limit
gyrus suspicious_program.bf --max-steps 1000000

# Or use a timeout
gyrus suspicious_program.bf --timeout 5000
```
