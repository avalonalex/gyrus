* Test file with multiple bracket errors
* This should report all errors at once

+++[  * Opening bracket 1 (no close)
>++[  * Opening bracket 2 (no close)
<-[   * Opening bracket 3 (no close)

* Three unmatched opening brackets above

+++]  * Closing bracket 1 (extra - will match one open)
]     * Closing bracket 2 (extra - will match one open)
]     * Closing bracket 3 (extra - will match one open)
]     * Closing bracket 4 (unmatched - no open for this!)
]     * Closing bracket 5 (unmatched - no open for this!)

* Should have 2 unmatched closing brackets
