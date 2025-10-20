* EOF no-change test
* Pre-set cell to 'X' (88) then try to read

++++++++++  * Set counter to 10 in cell 0
[           * Loop 10 times
  >++++++++  * Add 8 to cell 1 each time (10 * 8 = 80)
  <-        * Decrement counter
]
>++++++++   * Add 8 more to cell 1 (80 + 8 = 88 = 'X')
,           * Try to read into cell 1 (EOF with no-change keeps 88)
.           * Output cell 1 (should be 'X' = 88)
