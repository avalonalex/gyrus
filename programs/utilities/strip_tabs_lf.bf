* Strip tabs and linefeeds
*
* Removes ASCII 9 (tab) and ASCII 10 (linefeed) from input
* All other characters pass through unchanged
*
* Usage: echo -e "hello\tworld\n" | ferrous-cortex strip_tabs_lf.bf
* Output: helloworld
*
* Original from http://www.hevanet.com/cristofd/brainfuck/

,[---------[-[++++++++++.[-]]],]
