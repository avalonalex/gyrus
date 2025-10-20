This is a BrainFuck program with extensive comments.
Any text that isn't one of the 8 commands (><+-.,[]) is ignored.

Here's a simple program that prints 'A' (ASCII 65):

++++++++    Set cell 0 to 8
[           Start loop
  >++++     Move to cell 1 and add 4
  <-        Move back to cell 0 and decrement
]           Loop until cell 0 is 0 (runs 8 times, so cell 1 = 32)
>++.        Move to cell 1, add 2 (now 34), but we need 65...

Let me fix this to actually print 'A':

Actually, let's start over:
++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.

This is the classic approach - it prints 'H' actually.
For 'A' we need ASCII 65.
