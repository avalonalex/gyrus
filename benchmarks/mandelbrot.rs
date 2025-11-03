// Mandelbrot set renderer in Rust
// This program generates the same ASCII art as mandelbrot.bf
// Used for performance comparison between native Rust and BrainFuck interpreter

use std::time::Instant;

fn main() {
    let start = Instant::now();

    // Parameters matching the BrainFuck version exactly
    const WIDTH: i32 = 129;
    const HEIGHT: i32 = 48;
    const MAX_ITER: i32 = 127;

    // Coordinate ranges - these define the view into the complex plane
    // Tuned to match the Erik Bosman BF mandelbrot coordinates
    const X_MIN: f64 = -2.0;
    const X_MAX: f64 = 0.67;
    const Y_MIN: f64 = -1.15;
    const Y_MAX: f64 = 1.15;

    let dx = (X_MAX - X_MIN) / WIDTH as f64;
    let dy = (Y_MAX - Y_MIN) / HEIGHT as f64;

    for row in 0..HEIGHT {
        let cy = Y_MIN + row as f64 * dy;

        for col in 0..WIDTH {
            let cx = X_MIN + col as f64 * dx;

            // Mandelbrot iteration - standard algorithm
            let mut x = 0.0;
            let mut y = 0.0;
            let mut iteration = 0;

            while iteration < MAX_ITER {
                let xx = x * x;
                let yy = y * y;

                // Check if we've escaped (x^2 + y^2 > 4)
                if xx + yy > 4.0 {
                    break;
                }

                // Calculate new values: z = z^2 + c
                let xtemp = xx - yy + cx;
                y = 2.0 * x * y + cy;
                x = xtemp;

                iteration += 1;
            }

            // Map iteration count to ASCII characters (matching BF output)
            // The BF version outputs ASCII character based on 'A' + iteration
            // This gives us: A (1 iter), B (2 iter), ..., Z (26 iter), then continues
            // beyond Z into other ASCII characters for higher iteration counts
            let c = if iteration >= MAX_ITER {
                ' '
            } else if iteration == 0 {
                ' '
            } else {
                // Output 'A' + (iteration - 1), allowing it to go beyond 'Z'
                (b'A' + (iteration - 1) as u8) as char
            };

            print!("{}", c);
        }
        println!();
    }

    let elapsed = start.elapsed();
    eprintln!("\nRust execution time: {:?}", elapsed);
}
