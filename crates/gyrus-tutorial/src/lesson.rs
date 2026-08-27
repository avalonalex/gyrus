//! The course: what each lesson says, and what counts as having done it.

use crate::trace::{Ending, Trace};
use std::sync::LazyLock;

/// One thing a lesson asks of the learner's program.
///
/// A lesson carries a slice of these rather than a single tagged `Check`,
/// because criteria compose: two lessons already want a cell *and* an output,
/// and a combining variant for every pair does not scale past two. An empty
/// slice is a lesson that only wants reading.
#[derive(Debug, Clone)]
pub enum Criterion {
    /// These cells must hold these values when the program finishes. Cells not
    /// listed can hold anything.
    Cells(Vec<(usize, u8)>),
    /// The program must print exactly this.
    Output(String),
    /// The program must be no longer than this, counting non-whitespace
    /// characters — for the lessons whose point is brevity.
    Length(usize),
}

/// The result of checking an attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was asked for.
    Nothing,
    /// The attempt does what the lesson asked.
    Solved,
    /// It does not, and here is the first thing that is wrong.
    NotYet(String),
}

impl Verdict {
    /// Whether the lesson is satisfied.
    pub fn is_solved(&self) -> bool {
        matches!(self, Verdict::Solved | Verdict::Nothing)
    }
}

impl Criterion {
    /// Judge one recorded run against this criterion alone.
    fn evaluate(&self, trace: &Trace, source: &str) -> Option<String> {
        match self {
            Criterion::Cells(cells) => cells.iter().find_map(|&(address, expected)| {
                let actual = trace.memory.get(address).copied().unwrap_or(0);
                (actual != expected).then(|| {
                    format!("cell {address} holds {actual}, and the lesson asked for {expected}")
                })
            }),
            Criterion::Output(expected) => {
                let actual = String::from_utf8_lossy(&trace.output);
                (actual != expected.as_str()).then(|| {
                    format!(
                        "the program printed {:?}, and the lesson asked for {:?}",
                        actual, expected
                    )
                })
            }
            Criterion::Length(limit) => {
                let length = source.chars().filter(|c| !c.is_whitespace()).count();
                (length > *limit).then(|| {
                    format!("that works, but it is {length} characters and the limit is {limit}")
                })
            }
        }
    }
}

/// Judge one recorded run against everything a lesson asks of it.
pub fn evaluate(criteria: &[Criterion], trace: &Trace, source: &str) -> Verdict {
    if criteria.is_empty() {
        return Verdict::Nothing;
    }

    // A program that was cut off or failed has no final tape to check, and
    // saying "cell 1 holds 0" about a run that never got there would send the
    // learner looking in the wrong place.
    match &trace.ending {
        Ending::TooManySteps(limit) => {
            return Verdict::NotYet(format!(
                "the program was still running after {limit} steps — something never reaches zero"
            ));
        }
        Ending::Failed(message) => {
            let first = message.lines().next().unwrap_or("the program failed");
            return Verdict::NotYet(first.to_string());
        }
        Ending::Finished => {}
    }

    match criteria
        .iter()
        .find_map(|criterion| criterion.evaluate(trace, source))
    {
        Some(why) => Verdict::NotYet(why),
        None => Verdict::Solved,
    }
}

/// One lesson.
#[derive(Debug)]
pub struct Lesson {
    /// Short name, shown in the header and in `--list`.
    pub title: String,
    /// The explanation, in the left panel.
    pub body: String,
    /// What the learner is asked to do.
    pub task: String,
    /// The program the editor starts with — usually the example being explained.
    pub starter: String,
    /// A program that satisfies the check, revealed on request.
    pub answer: String,
    /// Nudges, revealed one at a time.
    pub hints: Vec<String>,
    /// What counts as done. Empty means the lesson only wants reading.
    pub criteria: Vec<Criterion>,
    /// What the starter provably does. Never shown to the learner and never
    /// read by the running binary: it exists so the tests can hold the body's
    /// prose to the code, and "run it and watch cell 1 reach 12" cannot
    /// quietly stop being true.
    #[cfg_attr(not(test), allow(dead_code))]
    pub shows: Shows,
    /// Input handed to the program's `,`.
    pub input: String,
    /// How many cells the tape has for this lesson.
    pub cells: usize,
}

/// What a lesson's starter is claimed to do, checked by the tests.
#[derive(Debug, Default, Clone)]
pub struct Shows {
    /// Whether the starter finishes or runs into the step cap.
    pub ending: Option<Expected>,
    /// Cells it leaves non-zero. Every cell not listed must be zero.
    pub cells: Option<Vec<(usize, u8)>>,
    /// Exactly what it prints.
    pub output: Option<String>,
    /// What it prints first, for the starters that never stop.
    pub output_prefix: Option<String>,
}

/// How a starter is expected to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    Finished,
    Capped,
}

/// How many steps a lesson program may run before the tutorial gives up.
///
/// Small on purpose: a lesson snippet that runs longer than this has gone
/// wrong, and lesson 12 is about that being the only answer available.
pub const STEP_LIMIT: usize = 20_000;

/// The course itself, compiled in and parsed at first use.
///
/// The lessons were a Rust table until the course grew past what is pleasant
/// to edit inside string literals. `include_str!` keeps the binary a single
/// file — the course is not something a user can lose or a packager can
/// forget — while the prose lives somewhere it can be written as prose.
///
/// What that costs is a failure mode the compiler used to cover: a malformed
/// course is a runtime panic rather than a build error. `the_course_parses`
/// below is what pays it back, and it runs on every `cargo test`.
pub static LESSONS: LazyLock<Vec<Lesson>> =
    LazyLock::new(|| parse(COURSE).unwrap_or_else(|why| panic!("course.toml is malformed: {why}")));

const COURSE: &str = include_str!("../course.toml");

/// One lesson under construction, before it is known to be complete.
#[derive(Default)]
struct Draft {
    title: Option<String>,
    body: Option<String>,
    task: Option<String>,
    starter: Option<String>,
    answer: Option<String>,
    hints: Option<Vec<String>>,
    cells: Option<usize>,
    input: Option<String>,
    solved_cells: Option<Vec<(usize, u8)>>,
    solved_output: Option<String>,
    solved_length: Option<usize>,
    shows: Shows,
}

impl Draft {
    fn finish(self, line: usize) -> Result<Lesson, String> {
        let title = self
            .title
            .ok_or_else(|| format!("line {line}: a lesson with no `title`"))?;
        let need = |what: &str, value: Option<String>| {
            value.ok_or_else(|| format!("{title}: no `{what}`"))
        };
        let mut criteria = Vec::new();
        if let Some(cells) = self.solved_cells {
            criteria.push(Criterion::Cells(cells));
        }
        if let Some(output) = self.solved_output {
            criteria.push(Criterion::Output(output));
        }
        if let Some(limit) = self.solved_length {
            criteria.push(Criterion::Length(limit));
        }
        Ok(Lesson {
            body: need("body", self.body)?,
            task: need("task", self.task)?,
            starter: need("starter", self.starter)?,
            answer: need("answer", self.answer)?,
            hints: self.hints.ok_or_else(|| format!("{title}: no `hints`"))?,
            cells: self.cells.ok_or_else(|| format!("{title}: no `cells`"))?,
            input: self.input.unwrap_or_default(),
            criteria,
            shows: self.shows,
            title,
        })
    }
}

/// `course.toml` in, lessons out.
///
/// A deliberately small parser rather than a dependency, for the reason
/// `gyrus-corpus` gives about the program manifest: it understands exactly
/// this file's shape and refuses everything else, so a misspelled key is an
/// error instead of a lesson quietly missing its check.
fn parse(source: &str) -> Result<Vec<Lesson>, String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut lessons = Vec::new();
    let mut draft: Option<Draft> = None;
    let mut index = 0;

    while index < lines.len() {
        let number = index + 1;
        let line = lines[index].trim();
        index += 1;

        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[lesson]]" {
            if let Some(previous) = draft.take() {
                lessons.push(previous.finish(number)?);
            }
            draft = Some(Draft::default());
            continue;
        }

        let Some((key, rest)) = line.split_once('=') else {
            return Err(format!("line {number}: {line:?} is not `key = value`"));
        };
        let (key, rest) = (key.trim(), rest.trim());
        let Some(draft) = draft.as_mut() else {
            return Err(format!("line {number}: `{key}` before any [[lesson]]"));
        };

        // A literal block runs to the delimiter that closes it, which sits at
        // the end of the last line of the text rather than on a line of its
        // own. TOML drops the newline after the opening delimiter, so this
        // way the value ends exactly where the text does and the parser and
        // the format agree about every byte.
        let value = if rest == TRIPLE {
            let mut body = String::new();
            loop {
                let Some(next) = lines.get(index) else {
                    return Err(format!("line {number}: `{key}` is never closed"));
                };
                index += 1;
                if let Some(last) = next.strip_suffix(TRIPLE) {
                    body.push_str(last);
                    break;
                }
                body.push_str(next);
                body.push('\n');
            }
            Value::Text(body)
        // A list may run over several lines; a hint long enough to want that
        // is exactly the hint most worth writing. Brackets are counted only
        // outside quotes, so a hint may contain one.
        } else if rest.starts_with('[') && !balanced(rest) {
            let mut list = rest.to_string();
            loop {
                let Some(next) = lines.get(index) else {
                    return Err(format!("line {number}: the list `{key}` is never closed"));
                };
                index += 1;
                list.push(' ');
                list.push_str(next.trim());
                if balanced(&list) {
                    break;
                }
            }
            value_of(&list, number)?
        } else {
            value_of(rest, number)?
        };

        match (key, value) {
            ("title", Value::Text(v)) => draft.title = Some(v),
            ("body", Value::Text(v)) => draft.body = Some(v),
            ("task", Value::Text(v)) => draft.task = Some(v),
            ("starter", Value::Text(v)) => draft.starter = Some(v),
            ("answer", Value::Text(v)) => draft.answer = Some(v),
            ("input", Value::Text(v)) => draft.input = Some(v),
            ("cells", Value::Number(v)) => draft.cells = Some(v),
            ("hints", Value::Strings(v)) => draft.hints = Some(v),
            ("solved_cells", Value::Pairs(v)) => draft.solved_cells = Some(v),
            ("solved_output", Value::Text(v)) => draft.solved_output = Some(v),
            ("solved_length", Value::Number(v)) => draft.solved_length = Some(v),
            ("shows_cells", Value::Pairs(v)) => draft.shows.cells = Some(v),
            ("shows_output", Value::Text(v)) => draft.shows.output = Some(v),
            ("shows_output_prefix", Value::Text(v)) => draft.shows.output_prefix = Some(v),
            ("shows_ending", Value::Text(v)) => {
                draft.shows.ending = Some(match v.as_str() {
                    "finished" => Expected::Finished,
                    "capped" => Expected::Capped,
                    other => {
                        return Err(format!(
                            "line {number}: shows_ending is {other:?}, and the only two are \
                             \"finished\" and \"capped\""
                        ));
                    }
                });
            }
            (key, _) => {
                return Err(format!(
                    "line {number}: `{key}` is not a key this course understands, or its value \
                     is the wrong shape"
                ));
            }
        }
    }

    match draft {
        Some(last) => lessons.push(last.finish(lines.len())?),
        None => return Err("no lessons in the file".to_string()),
    }
    Ok(lessons)
}

/// Whether a list's brackets close, counting only the ones outside quotes.
fn balanced(text: &str) -> bool {
    let (mut depth, mut quoted, mut escaped) = (0i32, false, false);
    for c in text.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// The delimiter a prose block opens and closes with.
const TRIPLE: &str = "'''";

/// The four shapes a value can have here.
enum Value {
    Text(String),
    Number(usize),
    Strings(Vec<String>),
    Pairs(Vec<(usize, u8)>),
}

fn value_of(text: &str, line: usize) -> Result<Value, String> {
    if let Some(inner) = quoted(text) {
        return Ok(Value::Text(unescape(inner, line)?));
    }
    if let Ok(number) = text.parse::<usize>() {
        return Ok(Value::Number(number));
    }
    let Some(inside) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
        return Err(format!(
            "line {line}: {text:?} is not a string, a number or a list"
        ));
    };
    let inside = inside.trim();
    if inside.is_empty() {
        return Ok(Value::Strings(Vec::new()));
    }
    if inside.starts_with('[') {
        let mut pairs = Vec::new();
        for item in inside.trim_end_matches(',').split("],") {
            let item = item.trim().trim_start_matches('[').trim_end_matches(']');
            let parts = item
                .split_once(',')
                .map(|(a, v)| (a.trim().parse::<usize>(), v.trim().parse::<u8>()));
            match parts {
                Some((Ok(address), Ok(value))) => pairs.push((address, value)),
                _ => {
                    return Err(format!("line {line}: {item:?} is not a [cell, value] pair"));
                }
            }
        }
        return Ok(Value::Pairs(pairs));
    }
    // A list of strings, split on the commas *between* them rather than on
    // any comma inside one, which a hint is entitled to contain.
    let mut strings = Vec::new();
    let mut rest = inside;
    loop {
        rest = rest.trim_start();
        let Some(end) = closing_quote(rest) else {
            return Err(format!("line {line}: {rest:?} is not a quoted string"));
        };
        strings.push(unescape(&rest[1..end], line)?);
        rest = rest[end + 1..].trim_start();
        match rest.strip_prefix(',') {
            // A comma with nothing after it is the trailing one a list is
            // allowed to end with, and is how a list written over several
            // lines usually ends.
            Some(more) if more.trim().is_empty() => break,
            Some(more) => rest = more,
            None if rest.is_empty() => break,
            None => {
                return Err(format!(
                    "line {line}: {rest:?} follows a string without a comma"
                ));
            }
        }
    }
    Ok(Value::Strings(strings))
}

/// The whole of `text` as one quoted string, or nothing.
fn quoted(text: &str) -> Option<&str> {
    let end = closing_quote(text)?;
    (end == text.len() - 1).then(|| &text[1..end])
}

/// Where the string starting at `text[0]` ends, honouring backslash escapes.
fn closing_quote(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Some(index),
            _ => index += 1,
        }
    }
    None
}

/// The escapes this file is allowed to use. Anything else is an error rather
/// than a backslash that silently survives into a lesson.
fn unescape(text: &str, line: usize) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => return Err(format!("line {line}: \\{other} is not an escape")),
            None => return Err(format!("line {line}: a backslash ends the string")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace;

    fn run(source: &str, lesson: &Lesson) -> Trace {
        trace::record(source, &lesson.input, lesson.cells, STEP_LIMIT)
            .unwrap_or_else(|error| panic!("{:?} does not parse: {error}", source))
    }

    /// Every lesson's answer has to satisfy that lesson's check.
    ///
    /// This is the claim most likely to rot: the checks and the answers are two
    /// separate pieces of prose, and editing one of them is exactly the kind of
    /// change nobody re-runs by hand.
    #[test]
    fn every_answer_solves_its_own_lesson() {
        for (index, lesson) in LESSONS.iter().enumerate() {
            let trace = run(&lesson.answer, lesson);
            let verdict = evaluate(&lesson.criteria, &trace, &lesson.answer);
            assert!(
                verdict.is_solved(),
                "lesson {index} ({}): the answer {:?} gives {verdict:?}",
                lesson.title,
                lesson.answer
            );
        }
    }

    /// Every starter program has to at least parse and run.
    ///
    /// Lesson 9's starter is deliberately an endless loop and lesson 12's is
    /// an endless loop on purpose, so hitting the step cap is allowed here —
    /// failing to parse, or crashing, is not.
    #[test]
    fn every_starter_parses_and_runs() {
        for (index, lesson) in LESSONS.iter().enumerate() {
            let trace = run(&lesson.starter, lesson);
            assert!(
                !matches!(trace.ending, Ending::Failed(_)),
                "lesson {index} ({}): the starter failed: {:?}",
                lesson.title,
                trace.ending
            );
        }
    }

    /// A lesson whose starter already satisfies it teaches nothing.
    ///
    /// The reading lessons are exempt: their check is `Explore`, so anything
    /// satisfies them by design.
    #[test]
    fn no_starter_is_already_the_answer() {
        for (index, lesson) in LESSONS.iter().enumerate() {
            if lesson.criteria.is_empty() {
                continue;
            }
            let trace = run(&lesson.starter, lesson);
            let verdict = evaluate(&lesson.criteria, &trace, &lesson.starter);
            assert!(
                !verdict.is_solved(),
                "lesson {index} ({}): the starter already solves it, so there is nothing to do",
                lesson.title
            );
        }
    }

    /// The course file has to parse, and say why if it does not.
    ///
    /// `LESSONS` panics on a malformed course, which in a shipped binary is
    /// the worst moment to find out. This is the moment instead.
    #[test]
    fn the_course_parses() {
        if let Err(why) = parse(COURSE) {
            panic!("course.toml does not parse: {why}");
        }
    }

    /// Every lesson's body says what its starter does. This checks it.
    ///
    /// Before the course moved to a file, a starter was only required to run
    /// without crashing — so prose like "watch cell 1 reach 12" was a claim
    /// nothing held anyone to, and editing the starter could falsify the
    /// paragraph beside it without failing a single test.
    #[test]
    fn every_starter_does_what_the_course_says() {
        for (index, lesson) in LESSONS.iter().enumerate() {
            let where_ = format!("lesson {index} ({})", lesson.title);
            let trace = run(&lesson.starter, lesson);

            let ending = lesson
                .shows
                .ending
                .unwrap_or_else(|| panic!("{where_}: no `shows_ending`, so nothing pins it"));
            match (ending, &trace.ending) {
                (Expected::Finished, Ending::Finished) => {}
                (Expected::Capped, Ending::TooManySteps(_)) => {}
                (expected, actual) => {
                    panic!("{where_}: the course says {expected:?}, and it was {actual:?}")
                }
            }

            if let Some(cells) = &lesson.shows.cells {
                for (address, value) in cells {
                    let actual = trace.memory.get(*address).copied().unwrap_or(0);
                    assert_eq!(
                        actual, *value,
                        "{where_}: the course says cell {address} ends at {value}, and it is {actual}"
                    );
                }
                // Everything the course did not list has to be zero, so a
                // starter cannot quietly start leaving something behind.
                for (address, actual) in trace.memory.iter().enumerate() {
                    if *actual != 0 && !cells.iter().any(|(listed, _)| *listed == address) {
                        panic!(
                            "{where_}: cell {address} ends at {actual}, and the course does not mention it"
                        );
                    }
                }
            }

            let printed = String::from_utf8_lossy(&trace.output).into_owned();
            if let Some(expected) = &lesson.shows.output {
                assert_eq!(&printed, expected, "{where_}: what the starter printed");
            }
            if let Some(prefix) = &lesson.shows.output_prefix {
                assert!(
                    printed.starts_with(prefix.as_str()),
                    "{where_}: the course says it starts by printing {prefix:?}, and it printed {:?}",
                    printed.chars().take(20).collect::<String>()
                );
            }
        }
    }

    /// A list may be written over several lines, with the trailing comma that
    /// makes adding to it a one-line diff.
    #[test]
    fn a_list_may_span_lines() {
        let course = concat!(
            "[[lesson]]\ntitle = \"t\"\ncells = 16\nstarter = \"\"\nanswer = \"+\"\n",
            "hints = [\n  \"first, with a comma in it\",\n  \"second [with a bracket]\",\n]\n",
            "task = '''\nt'''\nbody = '''\nb'''\n"
        );
        let lessons = parse(course).expect("this course is well formed");
        assert_eq!(
            lessons[0].hints,
            ["first, with a comma in it", "second [with a bracket]"]
        );
    }
    /// A key the parser does not know is an error, not a shrug.
    ///
    /// This is the whole reason for hand-writing the parser rather than
    /// reaching for a permissive one: `solved_cell` instead of `solved_cells`
    /// would otherwise be a lesson with no check that still looked complete.
    #[test]
    fn the_parser_refuses_a_key_it_does_not_know() {
        let course = "[[lesson]]\ntitle = \"x\"\nsolved_cell = [[0, 1]]\n";
        let why = parse(course).expect_err("a misspelled key has to be refused");
        assert!(why.contains("solved_cell"), "{why}");
    }

    /// So is a lesson that is missing something.
    #[test]
    fn the_parser_refuses_an_incomplete_lesson() {
        let course = "[[lesson]]\ntitle = \"Half a lesson\"\ncells = 16\n";
        let why = parse(course).expect_err("an incomplete lesson has to be refused");
        assert!(why.contains("Half a lesson"), "{why}");
    }

    /// A prose block keeps its newlines and drops the one after the delimiter.
    #[test]
    fn a_prose_block_is_read_verbatim() {
        let course = concat!(
            "[[lesson]]\ntitle = \"t\"\ncells = 16\nstarter = \"\"\nanswer = \"+\"\n",
            "hints = [\"h\"]\ntask = '''\nonce'''\nbody = '''\nfirst\n\nthird'''\n"
        );
        let lessons = parse(course).expect("this course is well formed");
        assert_eq!(lessons[0].body, "first\n\nthird");
        assert_eq!(lessons[0].task, "once");
    }
    /// Hints should not be empty strings, and answers should not be blank.
    #[test]
    fn each_lesson_is_filled_in() {
        for (index, lesson) in LESSONS.iter().enumerate() {
            assert!(!lesson.title.is_empty(), "lesson {index} has no title");
            assert!(!lesson.body.is_empty(), "lesson {index} has no body");
            assert!(!lesson.task.is_empty(), "lesson {index} has no task");
            assert!(!lesson.answer.is_empty(), "lesson {index} has no answer");
            assert!(!lesson.hints.is_empty(), "lesson {index} has no hints");
            assert!(
                lesson.hints.iter().all(|hint| !hint.is_empty()),
                "lesson {index} has an empty hint"
            );
        }
    }

    #[test]
    fn a_check_reports_which_cell_is_wrong() {
        let lesson = &LESSONS[1];
        let trace = run("+", lesson);
        match evaluate(&lesson.criteria, &trace, "+") {
            Verdict::NotYet(why) => {
                assert!(why.contains("cell 0"), "{why}");
                assert!(why.contains('7'), "{why}");
            }
            other => panic!("expected NotYet, got {other:?}"),
        }
    }

    #[test]
    fn a_program_that_never_ends_is_reported_as_such_and_not_as_a_wrong_cell() {
        let lesson = &LESSONS[1];
        let trace = run("+[]", lesson);
        match evaluate(&lesson.criteria, &trace, "+[]") {
            Verdict::NotYet(why) => assert!(why.contains("still running"), "{why}"),
            other => panic!("expected NotYet, got {other:?}"),
        }
    }

    #[test]
    fn a_length_budget_is_enforced_even_when_the_cells_are_right() {
        // Lesson 8 asks for 100 in cell 2 in under forty characters. A hundred
        // pluses and two moves gets the cells right and misses the point.
        let lesson = &LESSONS[8];
        let brute = format!(">>{}", "+".repeat(100));
        let trace = run(&brute, lesson);
        match evaluate(&lesson.criteria, &trace, &brute) {
            Verdict::NotYet(why) => assert!(why.contains("limit"), "{why}"),
            other => panic!("expected NotYet, got {other:?}"),
        }
    }
}
