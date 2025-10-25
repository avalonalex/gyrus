* Infinite Loop Error Example
* This program contains an infinite loop that never terminates
*
* Run with execution limits:
*   cargo run -- examples/errors/infinite_loop.bf --max-steps 10000
*   cargo run -- examples/errors/infinite_loop.bf --timeout 1000
*
* Without limits, this program will run forever!

* Set cell to a non-zero value
+

* Infinite loop: cell never becomes zero because we only increment
[
  +   * Increment (makes it worse!)
  +   * This loop will never exit naturally
]

* This code is never reached
.

* The error messages will show:
*
* With --max-steps 10000:
*   "Step limit exceeded: program executed 10000 steps"
*
* With --timeout 1000:
*   "Execution timeout: program exceeded 1000ms execution limit"
*
* Pro tip: Use --validate to get warnings about suspicious patterns
* like this before execution:
*   cargo run -- examples/errors/infinite_loop.bf --validate
