* Validation Warnings Example
* This program demonstrates various validation warnings
*
* Run with:
*   cargo run -- examples/errors/validation_warnings.bf --validate
*
* Use strict mode to treat warnings as errors:
*   cargo run -- examples/errors/validation_warnings.bf --strict
*
* IMPORTANT: This file contains intentional bugs for demonstration!
* --validate mode does NOT execute the program, so it's safe to run.
* These patterns would cause problems if executed.

* WARNING 1: Empty loop - does nothing
[]

* WARNING 2: Infinite loop - cell never becomes zero by incrementing
+[++]

* WARNING 3: Another infinite loop variant
+[+]

* WARNING 4: Extreme nesting (this is 12 levels deep)
* This can cause performance issues
[[[[[[[[[[[
  +
]]]]]]]]]]]

* These patterns are syntactically valid but likely indicate bugs
* or inefficient code that should be reviewed.
*
* Validation helps catch these issues BEFORE execution!
*
* Expected warnings:
* - "Empty loop at line X - does nothing"
* - "Potential infinite loop at line Y - cell only increments"
* - "Extreme nesting at line Z - 12 levels deep (performance warning)"
*
* To actually execute this (not recommended):
*   cargo run -- examples/errors/validation_warnings.bf
* This will hang in infinite loops!
