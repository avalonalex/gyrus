* Translate text to brainfuck that prints it
*
* Converts input text into BrainFuck code that outputs that text
* Uses + and - commands to reach each character's ASCII value
*
* Usage: echo "Hi" | gyrus text_to_bf.bf
* Output: BrainFuck code that prints "Hi"
*
* Original from http://www.hevanet.com/cristofd/brainfuck/

+++++[>+++++++++<-],[[>--.++>+<<-]>+.->[<.>-]<<,]
