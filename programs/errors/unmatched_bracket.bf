* Unmatched Bracket Error Example
* This program demonstrates bracket matching errors
*
* Run with:
*   cargo run -- examples/errors/unmatched_bracket.bf
*
* Expected: Parser error showing unmatched '[' with source context

+++[>++<]   * Valid loop
>+          * Move and increment

[>+++       * Opening bracket without matching close
            * This will cause a parse error

* The error message will show:
* - Exact line and column number
* - Source code context with caret (^) pointing to the error
* - Clear description of the problem
