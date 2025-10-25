* ROT13 Cipher
* Reads text from input and outputs ROT13 encoded version
* ROT13: Caesar cipher with shift of 13
* Letters A-Z become N-ZA-M
* Non-alphabetic characters pass through unchanged
*
* Usage: echo "Hello World" | ferrous-cortex rot13.bf
* Output: Uryyb Jbeyq
*
* ⚠️ WARNING: This is a STUB implementation that does not work correctly.
* ⚠️ This file needs to be replaced with a proper BrainFuck algorithm.
* ⚠️ Current implementation causes memory pointer errors.
*
* Algorithm (intended):
* - Read character
* - If A-Z or a-z: rotate by 13 positions
* - Else: output unchanged
* - Repeat until EOF

,                          Read first character
[                          Loop until EOF
  >+++++++++++++           Set cell 1 to 13 (ROT13 offset)
  [                        Rotation loop
    <                      Move to input character
    ------------------------------------------------ Subtract 48 (ASCII '0')
    <->>                   Decrement rotation counter
  ]
  <.                       Output rotated character
  ,                        Read next character
]
