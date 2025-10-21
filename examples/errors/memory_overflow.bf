* Memory Overflow Error Example
* This program attempts to access memory beyond the allowed bounds
*
* Run with:
*   cargo run -- examples/errors/memory_overflow.bf --memory-size 100
*
* Expected: Runtime error when pointer exceeds memory bounds
*
* With default memory (30000 cells), this would take a long time.
* Use --memory-size 100 to see the error quickly.

* Move right 100 times to exceed a 100-byte memory limit
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>

* Try to use this cell (will error with memory-size 100)
+

* The error message will show:
* - Instruction number where error occurred
* - Attempted cell access (e.g., cell 100)
* - Valid memory range (e.g., 0-99)
*
* Try different memory models:
*   --memory-model wrapping   (wraps around, no error)
*   --memory-model unbounded  (grows dynamically, no error)
