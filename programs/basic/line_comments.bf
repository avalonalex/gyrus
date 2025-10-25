* Line Comment Demo
* This program demonstrates the new * line comment feature
* Everything after * on a line is completely ignored!

* Print 'H' (ASCII 72)
++++++++++  * Cell 0 = 10
[           * Loop 10 times
  >+++++++  * Cell 1 += 7 each time
  <-        * Cell 0 -= 1
]           * Result: Cell 1 = 70, Cell 0 = 0
>++.        * Move to cell 1, add 2 (now 72), print 'H'

* You can safely use BF commands in comments after *
* Examples: >++<-- These won't execute!
* Even: ,.[] These are all ignored!

* Done!
