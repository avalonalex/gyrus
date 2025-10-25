* Copy input to output (cat)
*
* Usage: echo "Hello" | ferrous-cortex cat.bf
* Output: Hello
*
* Works with both EOF->0 and EOF->NoChange behaviors
* Original from http://www.hevanet.com/cristofd/brainfuck/

,[.[-],]
