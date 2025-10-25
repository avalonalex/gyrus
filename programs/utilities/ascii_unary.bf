* Show ASCII values of input in unary, separated by spaces
*
* Each character is output as N spaces, where N is the ASCII value
* Useful for checking implementation's newline behavior on input
*
* Usage: echo "ABC" | ferrous-cortex ascii_unary.bf
* Output: 65 spaces, then 66 spaces, then 67 spaces
*
* Original from http://www.hevanet.com/cristofd/brainfuck/

++++[>++++++++<-],[[>+.-<-]>.<,]
