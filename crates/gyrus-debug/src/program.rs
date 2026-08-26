//! The program under debug, and the two maps that connect it to its source.
//!
//! The interpreter talks in flat instruction indices; the user talks in lines
//! and columns. `DebugInfo` provides one direction of that translation, so this
//! builds the other one once at load time.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gyrus::{BfError, DebugInfo, Instruction, parse_with_debug};
use gyrus_tui::{Position, SourceDocument};

/// A loaded BrainFuck program with everything the debugger needs to show it.
pub struct Program {
    /// Path the source came from.
    pub path: PathBuf,
    /// Source text, split and syntax-classified for the source panel.
    pub document: SourceDocument,
    /// The parsed AST.
    pub instructions: Vec<Instruction>,
    /// Instruction index to source location, from the parser.
    pub debug: DebugInfo,
    /// Instruction index to source position. Dense: the parser records one
    /// location per instruction, numbered from zero.
    positions: Vec<Position>,
    /// The inverse map, so a cursor can name an instruction.
    indices: BTreeMap<Position, usize>,
}

impl Program {
    /// Parse `path`, keeping debug symbols.
    pub fn load(path: &Path) -> Result<Self, BfError> {
        let source = std::fs::read_to_string(path).map_err(|source| BfError::FileError {
            path: path.to_path_buf(),
            source,
            hint: format!(
                "Make sure the file exists and you have permission to read it. \
                 Current path: {}",
                path.display()
            ),
        })?;
        Self::from_source(path.to_path_buf(), &source)
    }

    /// Parse `source`, attributing it to `path`.
    pub fn from_source(path: PathBuf, source: &str) -> Result<Self, BfError> {
        let (instructions, debug) = parse_with_debug(source)?;

        let mut positions = Vec::with_capacity(debug.len());
        let mut indices = BTreeMap::new();
        for index in 0..debug.len() {
            let Some(location) = debug.lookup(index) else {
                break;
            };
            let position = (location.line, location.column);
            positions.push(position);
            indices.insert(position, index);
        }

        Ok(Self {
            path,
            document: SourceDocument::new(source),
            instructions,
            debug,
            positions,
            indices,
        })
    }

    /// File name for panel titles.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// How many instructions the program has.
    pub fn instruction_count(&self) -> usize {
        self.positions.len()
    }

    /// Where instruction `index` sits in the source.
    pub fn position(&self, index: usize) -> Option<Position> {
        self.positions.get(index).copied()
    }

    /// The instruction at an exact source position.
    pub fn index_at(&self, position: Position) -> Option<usize> {
        self.indices.get(&position).copied()
    }

    /// The instruction nearest `position` on the same line.
    ///
    /// A cursor lands wherever the arrow keys left it, which is usually a
    /// comment character. Snapping to the nearest instruction on that line is
    /// what makes `b` do the obvious thing instead of nothing.
    pub fn nearest_on_line(&self, position: Position) -> Option<(Position, usize)> {
        let (line, column) = position;
        let after = self
            .indices
            .range((line, column)..(line + 1, 0))
            .next()
            .map(|(p, i)| (*p, *i));
        let before = self
            .indices
            .range((line, 0)..(line, column))
            .next_back()
            .map(|(p, i)| (*p, *i));
        match (before, after) {
            (Some(b), Some(a)) => {
                if column - b.0.1 <= a.0.1 - column {
                    Some(b)
                } else {
                    Some(a)
                }
            }
            (Some(b), None) => Some(b),
            (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }

    /// The first instruction at or after `position`, wrapping to the start.
    ///
    /// Used to move the cursor between instructions rather than characters.
    pub fn next_instruction(&self, position: Position) -> Option<Position> {
        self.indices
            .range((position.0, position.1 + 1)..)
            .next()
            .map(|(p, _)| *p)
            .or_else(|| self.indices.keys().next().copied())
    }

    /// The last instruction before `position`, wrapping to the end.
    pub fn previous_instruction(&self, position: Position) -> Option<Position> {
        self.indices
            .range(..position)
            .next_back()
            .map(|(p, _)| *p)
            .or_else(|| self.indices.keys().next_back().copied())
    }

    /// The half-open instruction range a loop occupies, given the index of its
    /// `[`. Returns `None` when `index` is not a loop head.
    ///
    /// This is what "step over" needs: the loop's own extent, not its nesting
    /// depth, because the depth at `[` is the same before the loop and after it.
    pub fn loop_extent(&self, index: usize) -> Option<(usize, usize)> {
        let metadata = self.debug.get_loop_metadata(index)?;
        Some((index, index + metadata.body_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Program {
        Program::from_source(PathBuf::from("test.bf"), source).expect("parses")
    }

    #[test]
    fn every_instruction_maps_to_a_position_and_back() {
        let program = program("+>\n<-");
        assert_eq!(program.instruction_count(), 4);
        for index in 0..program.instruction_count() {
            let position = program.position(index).expect("position");
            assert_eq!(program.index_at(position), Some(index));
        }
        assert_eq!(program.position(0), Some((1, 1)));
        assert_eq!(program.position(2), Some((2, 1)));
    }

    #[test]
    fn comments_hold_no_instructions() {
        // `*` starts a line comment, so the `+` after it is text, not code.
        let program = program("+ * +");
        assert_eq!(program.instruction_count(), 1);
        assert_eq!(program.index_at((1, 5)), None);
    }

    #[test]
    fn the_cursor_snaps_to_the_nearest_instruction_on_its_line() {
        // Columns 3 and 5 hold code; everything else on the line is comment.
        let program = program("  + >");
        assert_eq!(program.nearest_on_line((1, 1)), Some(((1, 3), 0)));
        assert_eq!(program.nearest_on_line((1, 5)), Some(((1, 5), 1)));
        // A tie goes to the instruction before the cursor.
        assert_eq!(program.nearest_on_line((1, 4)), Some(((1, 3), 0)));
    }

    #[test]
    fn snapping_stays_on_the_cursors_line() {
        let program = program("comment only\n+");
        assert_eq!(program.nearest_on_line((1, 1)), None);
        assert_eq!(program.nearest_on_line((2, 1)), Some(((2, 1), 0)));
    }

    #[test]
    fn instruction_navigation_skips_comment_text_and_wraps() {
        let program = program("+   >");
        assert_eq!(program.next_instruction((1, 1)), Some((1, 5)));
        assert_eq!(program.next_instruction((1, 5)), Some((1, 1)));
        assert_eq!(program.previous_instruction((1, 5)), Some((1, 1)));
        assert_eq!(program.previous_instruction((1, 1)), Some((1, 5)));
    }

    #[test]
    fn a_loops_extent_covers_its_bracket_and_body_but_not_what_follows() {
        // + [ - > + < ] +   ->  indices 0..=6, with `]` costing nothing.
        let program = program("+[->+<]+");
        assert_eq!(program.loop_extent(1), Some((1, 6)));
        assert_eq!(program.position(6), Some((1, 8)));
        // Only the `[` starts a loop.
        assert_eq!(program.loop_extent(0), None);
        assert_eq!(program.loop_extent(2), None);
    }

    #[test]
    fn a_nested_loops_extent_sits_inside_its_parents() {
        let program = program("[[+]]");
        let outer = program.loop_extent(0).expect("outer loop");
        let inner = program.loop_extent(1).expect("inner loop");
        assert!(
            outer.0 <= inner.0 && inner.1 <= outer.1,
            "{outer:?} {inner:?}"
        );
    }
}
