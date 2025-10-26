* Test EOF behavior: read from empty input and output the result
* This tests what value the cell gets when EOF is encountered
* Expected behavior depends on EofBehavior setting:
* - SetZero: cell becomes 0
* - SetNegOne: cell becomes 255
* - NoChange: cell stays at previous value
* - Error: returns error

, Read byte (will be EOF since input is empty)
. Output the cell value
