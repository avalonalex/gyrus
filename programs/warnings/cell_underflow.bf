* Cell Underflow Example
*
* This program demonstrates cell underflow warnings in wrapping mode.
* When a cell at 0 is decremented, it wraps to 255.
*
* Expected output: Three decreasing values (255, 254, 253)
* Runtime warnings: 3 cell underflow events at each decrement from 0

* Cell starts at 0
* Decrement to get 255
-.

* Move to next cell (also 0), decrement twice to get 254
>--.

* Move to next cell (also 0), decrement three times to get 253
>---.
