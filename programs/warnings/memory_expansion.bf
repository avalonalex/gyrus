* Memory Expansion Example
*
* This program demonstrates memory expansion warnings in unbounded mode.
* Memory starts small and grows as the pointer moves beyond current bounds.
*
* Run with: --memory-model unbounded --unbounded-initial 5 --unbounded-max 20
*
* Expected behavior: Memory expands from 5 bytes up to 15 bytes
* Runtime warnings: Multiple memory expansion events

* Move right beyond initial memory (5 bytes)
* Each move past the boundary triggers an expansion warning
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
