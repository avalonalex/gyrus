* Cell Overflow Example
*
* This program demonstrates cell overflow warnings in wrapping mode.
* When a cell reaches 255 and is incremented, it wraps to 0.
*
* Expected output: Three asterisks "***"
* Runtime warnings: 3 cell overflow events

* Cell 0: Set to 255, increment to 0 (overflow), add 42 for '*'
-+++++++++++++++++++++++++++++++++++++++++++.

* Cell 1: Set to 255, increment to 0 (overflow), add 42 for '*'
>-+++++++++++++++++++++++++++++++++++++++++++.

* Cell 2: Set to 255, increment to 0 (overflow), add 42 for '*'
>-+++++++++++++++++++++++++++++++++++++++++++.
