//! Reading a tape for display.
//!
//! Every panel that shows memory asks the same three questions — what is under
//! the cursor, what does this byte look like, and what changed since last time
//! — and every one of them used to answer separately. The cursor question in
//! particular has one correct answer, in [`MemoryAddress::index`], and it is
//! not "cast to `usize` and hope".

use std::collections::HashSet;

use gyrus::MemoryAddress;

/// The value under a signed cursor, or `None` when the cursor is off the tape.
///
/// The cursor is signed because the tape contract lets it leave the tape;
/// only reading or writing out there is an error.
pub fn cell_under(memory: &[u8], pointer: isize) -> Option<u8> {
    MemoryAddress::new(pointer)
        .index(memory.len())
        .map(|index| memory[index])
}

/// Whether the cursor is sitting exactly on `address`.
pub fn points_at(pointer: isize, address: usize) -> bool {
    pointer >= 0 && pointer as usize == address
}

/// The character a byte stands for, or `placeholder` when it does not print.
///
/// The placeholder is a parameter because the panels disagree about it: a hex
/// dump's ASCII sidebar wants `.`, and the tape strip wants `·`.
pub fn printable(byte: u8, placeholder: char) -> char {
    if (0x20..0x7f).contains(&byte) {
        byte as char
    } else {
        placeholder
    }
}

/// Cells whose value differs between two tapes.
///
/// Handles `after` being longer, which is what an unbounded tape that just grew
/// looks like: everything past the old end is new if it is not zero.
pub fn changed_cells(before: &[u8], after: &[u8]) -> HashSet<usize> {
    let mut changed: HashSet<usize> = before
        .iter()
        .zip(after.iter())
        .enumerate()
        .filter(|(_, (old, new))| old != new)
        .map(|(address, _)| address)
        .collect();
    for (address, &value) in after.iter().enumerate().skip(before.len()) {
        if value != 0 {
            changed.insert(address);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_off_either_end_of_the_tape_has_no_cell() {
        let memory = [1u8, 2, 3];
        assert_eq!(cell_under(&memory, 1), Some(2));
        assert_eq!(cell_under(&memory, -1), None);
        assert_eq!(cell_under(&memory, 3), None);
    }

    #[test]
    fn printable_bytes_pass_through_and_the_rest_do_not() {
        assert_eq!(printable(b'A', '.'), 'A');
        assert_eq!(printable(b' ', '.'), ' ');
        assert_eq!(printable(0x7f, '.'), '.');
        assert_eq!(printable(0, '·'), '·');
    }

    #[test]
    fn a_diff_reports_only_the_cells_that_moved() {
        assert_eq!(changed_cells(&[0, 1, 2], &[0, 9, 2]), HashSet::from([1]));
    }

    #[test]
    fn a_tape_that_grew_counts_its_new_non_zero_cells() {
        // What an unbounded tape looks like after it expands.
        assert_eq!(changed_cells(&[1, 2], &[1, 2, 0, 7]), HashSet::from([3]));
    }
}
