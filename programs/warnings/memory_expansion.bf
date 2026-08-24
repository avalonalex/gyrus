* Memory Expansion Example
*
* This program demonstrates memory expansion warnings in unbounded mode.
* Memory starts small and grows when a cell beyond its bounds is used.
*
* Run with: --memory-model unbounded --unbounded-initial 5 --unbounded-max 20
*
* Expected behavior: Memory expands from 5 bytes up to 15 bytes
* Runtime warnings: Multiple memory expansion events

* Move right beyond initial memory (5 bytes)
* Moving past the boundary expands nothing; the write that follows is what
* grows the tape, and what the expansion warning points at
>>>>>>

* Write a marker at cell 6
+

* Continue moving to cell 10
>>>>

* Write another marker
++

* Move to cell 14
>>>>

* Write final marker
+++

* Print all markers by moving back
<<<<.
<<<<.
<<<<<<.
