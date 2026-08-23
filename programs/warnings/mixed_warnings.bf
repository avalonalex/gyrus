* Mixed Warnings Example
*
* This program demonstrates multiple types of runtime warnings:
* - Cell underflow (0 - 1 = 255)
* - Cell overflow (255 + 1 = 0)
* - Memory expansion (when using unbounded mode)
*
* Run with default settings to see cell warnings:
*   gyrus programs/warnings/mixed_warnings.bf
*
* Run with unbounded memory to see all warning types:
*   gyrus programs/warnings/mixed_warnings.bf --memory-model unbounded --unbounded-initial 3 --unbounded-max 20
*
* Expected output: "ABC" (65, 66, 67)

* Cell 0: Underflow, overflow, then add 65 for 'A'
-++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++.

* Cell 1: Underflow, overflow, then add 66 for 'B'
>-+++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++.

* Cell 2: Underflow, overflow, then add 67 for 'C'
>-++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++.

* Move to cell 5 (triggers expansion if unbounded with initial < 6)
>>>

* Overflow: decrement from 0 to 255, then increment to 0
-+
