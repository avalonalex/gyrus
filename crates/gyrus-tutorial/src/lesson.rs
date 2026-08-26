//! The course: what each lesson says, and what counts as having done it.

use crate::trace::{Ending, Trace};

/// What a lesson wants the learner's program to have done.
#[derive(Debug, Clone, Copy)]
pub enum Check {
    /// Nothing to satisfy. Read it, try things, move on.
    Explore,
    /// These cells must hold these values when the program finishes. Cells not
    /// listed can hold anything.
    Cells(&'static [(usize, u8)]),
    /// The program must print exactly this.
    Output(&'static str),
    /// Both of the above.
    Both(&'static [(usize, u8)], &'static str),
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

impl Check {
    /// Judge one recorded run.
    pub fn evaluate(&self, trace: &Trace, source: &str, max_length: Option<usize>) -> Verdict {
        if matches!(self, Check::Explore) {
            return Verdict::Nothing;
        }

        // A program that was cut off or failed has no final tape to check, and
        // saying "cell 1 holds 0" about a run that never got there would send
        // the learner looking in the wrong place.
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

        if let Some(limit) = max_length {
            let length = source.chars().filter(|c| !c.is_whitespace()).count();
            if length > limit {
                return Verdict::NotYet(format!(
                    "that works, but it is {length} characters and the limit is {limit}"
                ));
            }
        }

        let cells = match self {
            Check::Cells(cells) | Check::Both(cells, _) => Some(*cells),
            _ => None,
        };
        let expected_output = match self {
            Check::Output(text) | Check::Both(_, text) => Some(*text),
            _ => None,
        };

        if let Some(cells) = cells {
            for &(address, expected) in cells {
                let actual = trace.memory.get(address).copied().unwrap_or(0);
                if actual != expected {
                    return Verdict::NotYet(format!(
                        "cell {address} holds {actual}, and the lesson asked for {expected}"
                    ));
                }
            }
        }

        if let Some(expected) = expected_output {
            let actual = String::from_utf8_lossy(&trace.output);
            if actual != expected {
                return Verdict::NotYet(format!(
                    "the program printed {:?}, and the lesson asked for {:?}",
                    actual, expected
                ));
            }
        }

        Verdict::Solved
    }
}

/// One lesson.
pub struct Lesson {
    /// Short name, shown in the header and in `--list`.
    pub title: &'static str,
    /// The explanation, in the left panel.
    pub body: &'static str,
    /// What the learner is asked to do.
    pub task: &'static str,
    /// The program the editor starts with — usually the example being explained.
    pub starter: &'static str,
    /// A program that satisfies the check, revealed on request.
    pub answer: &'static str,
    /// Nudges, revealed one at a time.
    pub hints: &'static [&'static str],
    /// What counts as done.
    pub check: Check,
    /// Input handed to the program's `,`.
    pub input: &'static str,
    /// How many cells the tape has for this lesson.
    pub cells: usize,
    /// A character budget, when the point of the lesson is brevity.
    pub max_length: Option<usize>,
}

/// How many steps a lesson program may run before the tutorial gives up.
///
/// Small on purpose: a lesson snippet that runs longer than this has gone
/// wrong, and lesson 12 is about that being the only answer available.
pub const STEP_LIMIT: usize = 20_000;

/// The course, in order.
pub const LESSONS: &[Lesson] = &[
    Lesson {
        title: "Welcome",
        body: "\
BrainFuck gives you a tape of numbered cells, every one of them zero, and a pointer sitting on cell 0.

There are eight commands, and they are the entire language:

  +   add one to the cell under the pointer
  -   subtract one
  >   move the pointer one cell right
  <   move it one cell left
  .   print the cell as a character
  ,   read a character into the cell
  [   if the cell is zero, jump past the matching ]
  ]   jump back to the matching [

Every other character is a comment. That is why BrainFuck programs can be hidden inside English, and why a typo is silently ignored rather than reported.

Those eight commands are enough to compute anything any computer can compute. The rest of this course is about why that is true and what it costs.",
        task: "Type a single + and run it with ctrl-r. Watch cell 0.",
        starter: "",
        answer: "+",
        hints: &["One character. The tape starts at zero, and + adds one."],
        check: Check::Cells(&[(0, 1)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Counting",
        body: "\
+ and - are the whole of arithmetic. There is no way to write the number seven. You write seven pluses.

Run the program below, then press tab and walk through it with the arrow keys. Watch the value climb and then drop back.

A cell holds one byte: 0 to 255. Add one to 255 and you are at 0 again. Subtract one from 0 and you are at 255. Nothing complains — the wheel simply turns. gyrus can be told to complain instead, with --cell-model checked, which is how you find the bug where you meant to subtract four and subtracted five.",
        task: "Leave 7 in cell 0.",
        starter: "++++-",
        answer: "+++++++",
        hints: &[
            "Seven pluses is the direct route.",
            "So is nine pluses and two minuses. Both are right; one is shorter.",
        ],
        check: Check::Cells(&[(0, 7)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "The pointer",
        body: "\
> and < move the pointer. They change nothing on the tape; they change which cell + - . and , are talking about.

Run +>++>+++ and step through it. The ▲ under the tape moves, and each cell keeps whatever it was given after the pointer leaves.

The pointer is the only way this language has of pointing at anything. There are no names. \"Cell 3 is the loop counter\" is a fact you keep in your head, and losing track of where the pointer is left off is the most common way a BrainFuck program goes wrong. You will do it in lesson 9.",
        task: "Now reverse it: 3 in cell 0, 2 in cell 1, 1 in cell 2.",
        starter: "+>++>+++",
        answer: "+++>++>+",
        hints: &["Only the number of pluses in each group has to change."],
        check: Check::Cells(&[(0, 3), (1, 2), (2, 1)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Loops",
        body: "\
[ and ] are the only branch and the only jump in the language.

  [   if the cell under the pointer is zero, skip past the ]
  ]   go back to the [

So a loop runs while the current cell is not zero, which means the body has to change that cell or the loop never ends.

Run ++[>+<-] and step through it slowly, watching two cells:

  cell 0 counts down    2, 1, 0
  cell 1 counts up      0, 1, 2

When cell 0 reaches zero the [ stops jumping back.

So what did it do? It moved the value. Not copied — moved. Cell 0 is empty at the end, and that is the price of the simplest loop in the language. Getting a copy instead takes lesson 10.",
        task: "Move a 5 from cell 0 into cell 2 rather than cell 1.",
        starter: "++[>+<-]",
        answer: "+++++[>>+<<-]",
        hints: &[
            "Start with five pluses instead of two.",
            "Two > to reach cell 2 means two < to get back to the counter.",
            "Forget the second < and the ] will be testing the wrong cell.",
        ],
        check: Check::Cells(&[(0, 0), (2, 5)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Clearing",
        body: "\
How do you set a cell to zero when you do not know what is in it?

  [-]

Read it as a sentence: while this cell is not zero, subtract one. It lands on zero and stops, whatever it started at.

You will see [-] constantly. gyrus's optimizer recognises it by shape and replaces the entire loop with a single store — run gyrus-tool optimize on a program and look for Zero in the output.

Its evil twin is [+]. That terminates too, by climbing to 255 and wrapping round to 0, which takes 256 iterations to do what [-] does in as many as the cell needs. gyrus-tool validate warns about it, and under --cell-model checked it is not a slow program but a failing one.",
        task: "Set cell 0 to 5 and then clear it, and leave 9 in cell 1.",
        starter: "+++++",
        answer: "+++++[-]>+++++++++",
        hints: &[
            "Two steps: build the 5, then clear it with [-].",
            "Then move right and count to nine.",
        ],
        check: Check::Cells(&[(0, 0), (1, 9)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Multiplication",
        body: "\
A loop that adds to a second cell on every pass multiplies.

  +++[>++++<-]

Cell 0 counts three times; each pass adds four to cell 1. Three fours is twelve.

What is in cell 0 afterwards? Zero. The counter is consumed. Multiplication always costs you the multiplier here, and keeping it means copying it first — which is lesson 10, and which is why BrainFuck programs are mostly bookkeeping.

gyrus recognises this shape too. The optimizer calls it MultiplyAdd and computes the result without looping at all, which is why a program that spends its life in loops like this one runs far faster than its step count suggests it should.",
        task: "Compute 6 x 7 and leave 42 in cell 1.",
        starter: "+++[>++++<-]",
        answer: "++++++[>+++++++<-]",
        hints: &[
            "Six in the counter, seven added each pass.",
            "Or seven in the counter and six added — multiplication does not mind.",
        ],
        check: Check::Cells(&[(1, 42)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Why this is enough",
        body: "\
Look at what six lessons of eight characters have built:

  a value            a cell
  a variable         a cell you decided to call something
  assignment         [-] and then that many +
  addition           [>+<-]
  multiplication     a loop that adds
  unbounded memory   the tape keeps going

A language is Turing complete when it has unbounded storage, a way to branch on a value, and a way to repeat. You have all three, and you have not used anything that was not in lesson 3.

The conclusion is not that BrainFuck is powerful. It is that power is cheap. Anything computable is computable here — slowly, and unreadably, but computable. Your compiler's back end and your CPU's instruction set are doing the same work against the same ceiling, with better ergonomics.

Everything after this lesson is ergonomics.",
        task: "Nothing to solve. Run whatever you like, then press ctrl-n.",
        starter: "+++[>++++<-]>.",
        answer: "+++[>++++<-]>.",
        hints: &["There is nothing to get right here. ctrl-n moves on."],
        check: Check::Explore,
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Input and output",
        body: "\
. prints the cell under the pointer as a character. , reads one into it.

Characters are numbers: A is 65, a is 97, a space is 32, a newline is 10. So printing A means getting 65 into a cell and then saying . — and the short way to 65 is not sixty-five pluses:

  ++++++++[>++++++++<-]>+.

Eight eights is 64, and then one more. Run it.

, reads a byte from the program's input. What happens when there is none left is a choice rather than a law, and implementations disagree: gyrus can give you 0, give you 255, leave the cell untouched, or stop with an error, under --eof-behavior. Here it gives you 0. A program that reads input and does not agree with its interpreter about this will read one byte too many and loop.",
        task: "Print Hi. H is 72 and i is 105.",
        starter: "++++++++[>++++++++<-]>+.",
        answer: "++++++++[>+++++++++<-]>.+++++++++++++++++++++++++++++++++.",
        hints: &[
            "Eight nines is 72, and 72 is H.",
            "i is 105, which is 33 more than H. You are already on the right cell.",
        ],
        check: Check::Output("Hi"),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Nested loops",
        body: "\
A loop body can contain another loop, which is how you get large numbers without typing them.

  ++++++++++[>++++++++++<-]

Ten tens: cell 1 ends at 100, and you typed twenty-five characters instead of a hundred.

The outer counter is consumed on the way, as always, so a two-level nest needs one cell per level plus one for the answer. Keeping track of which cell is counting what is the entire difficulty, and it does not get easier — it is why lesson 10 exists and why real BrainFuck programs are written with a diagram of the tape beside them.",
        task: "Leave 100 in cell 2, in under 40 characters.",
        starter: "++++++++++[>++++++++++<-]",
        answer: "++++++++++[>>++++++++++<<-]",
        hints: &[
            "The starter puts 100 in cell 1. You want it one cell further right.",
            "Two > inside the loop means two < before the -.",
        ],
        check: Check::Cells(&[(2, 100)]),
        input: "",
        cells: 16,
        max_length: Some(40),
    },
    Lesson {
        title: "Making a decision",
        body: "\
There is no if. There is [ , which is an if that repeats — so an if is a loop you make sure cannot go round a second time:

  [ clear the cell, then do the thing ]

The program below builds the letter y in cell 1, puts a 3 in cell 0 as a flag, and is meant to print y once because that flag is not zero.

Run it. It prints y several thousand times and the tutorial cuts it off.

Nothing in the loop body ever touches cell 0, so lesson 3's rule bites: a loop runs while its cell is not zero, and this one has no way to become zero. Clearing the flag is the whole of what turns a loop into an if.

The other way to get this wrong is subtler, and you will meet it soon enough. The ] tests whatever cell the pointer is on when it is reached, not the cell the [ tested. Drop the < and the loop starts asking a different cell every time round, and the pointer walks off down the tape.

For a real else you need a second cell: set an else-flag to 1, clear it inside the then-branch, and follow with a loop on the else-flag.",
        task: "Make it print y exactly once and stop.",
        starter: "+++++++++++[>+++++++++++<-]+++[>.<]",
        answer: "+++++++++++[>+++++++++++<-]+++[[-]>.<]",
        hints: &[
            "Lesson 3: a loop runs while its cell is not zero. Which cell does this one test?",
            "Nothing inside the body changes cell 0, so the test never changes its answer.",
            "[ [-] ... ] clears the flag on the way in, so the body runs once.",
        ],
        // The flag has to end up cleared as well: a program that prints y and
        // leaves the flag set is one that got out of the loop some other way.
        check: Check::Both(&[(0, 0)], "y"),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Copying",
        body: "\
Moving a value takes one loop. Copying one takes a spare cell and a second pass:

  [>+>+<<-]      empty cell 0 into cells 1 and 2
  >>[<<+>>-]     empty cell 2 back into cell 0

Cell 2 was scratch. Cell 0 ends with what it started with, cell 1 has the copy, and the scratch cell is zero again — which matters, because the next thing you write will assume it is.

This is what passes for a subroutine here: a shape you write out again every time you need it, with the cell numbers adjusted by hand. There is no call, no return, and no arguments. Only a pattern, and the discipline to keep your tape straight.

That is also the argument for the macro preprocessor gyrus has a design for and no code behind. Until it exists, you type the pattern out.",
        task: "Leave 5 in each of cells 0, 1 and 2. Use cell 3 as scratch.",
        starter: "+++++[>+>+<<-]",
        answer: "+++++[>+>+>+<<<-]>>>[<<<+>>>-]",
        hints: &[
            "One pass can fill three cells, not two.",
            "Then move one of them back into cell 0 with a second loop.",
            "Every > inside a loop body needs its < before the ].",
        ],
        check: Check::Cells(&[(0, 5), (1, 5), (2, 5)]),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "Walking the tape",
        body: "\
A list is a run of consecutive cells, and the pointer is the index. To walk one until it runs out, use a zero cell as the end marker:

  [>]     move right until you land on a zero
  [<]     move left until you land on a zero

Read [>] with lesson 3 in mind and it is obvious: while this cell is not zero, move right. It is a loop whose body never touches a value.

gyrus recognises both. The optimizer calls them SeekRight and SeekLeft and does the whole walk as one operation rather than one step per cell, which is the difference between scanning a long tape and scanning it slowly.

Building a string is the same idea run backwards: fill a run of cells with character codes, then walk it printing as you go.",
        task: "Print ABC — 65, 66, 67 — by building the codes with a loop.",
        starter: "++++++++++++++++[>++++>++++>++++<<<-]",
        answer: "++++++++++++++++[>++++>++++>++++<<<-]>+.>++.>+++.",
        hints: &[
            "The starter leaves 64 in each of cells 1, 2 and 3.",
            "64 is one less than A, two less than B, three less than C.",
            "Move right, add the difference, print. Three times.",
        ],
        check: Check::Output("ABC"),
        input: "",
        cells: 16,
        max_length: None,
    },
    Lesson {
        title: "The halting problem",
        body: "\
In lesson 9 you wrote a program that did not stop, and the tutorial cut it off after a fixed number of steps and said so.

That cap is not laziness. No program can read an arbitrary BrainFuck program and decide whether it halts. Turing proved it in 1936, and the proof does not care which language is being asked about — only that the language can express a program that reads another program's verdict and does the opposite.

BrainFuck can express that. It is the same property that made it Turing complete in lesson 6. The power and the undecidability are one fact seen from two sides: you cannot have a language that can compute anything and also always answer questions about what it will do.

So every tool here has a cutoff where you might have wanted an answer:

  gyrus --max-steps N     stop after N instructions
  gyrus --timeout MS      stop after a wall-clock deadline
  gyrus-tool validate     warns about loops it can prove never end,
                          and says nothing about the rest

That last one is the honest shape of every such tool. It catches the cases it can see and makes no claim about the others, which is not a limitation anyone is going to fix.

Below is a cell set to one and a loop with an empty body that can never change it. Run it.",
        task: "Run it, watch the step cap stop it, and you are done.",
        starter: "+[]",
        answer: "+[]",
        hints: &["Nothing to solve. ctrl-r runs it; the cap does the rest."],
        check: Check::Explore,
        input: "",
        cells: 16,
        max_length: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace;

    fn run(source: &str, lesson: &Lesson) -> Trace {
        trace::record(source, lesson.input, lesson.cells, STEP_LIMIT)
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
            let trace = run(lesson.answer, lesson);
            let verdict = lesson
                .check
                .evaluate(&trace, lesson.answer, lesson.max_length);
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
            let trace = run(lesson.starter, lesson);
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
            if matches!(lesson.check, Check::Explore) {
                continue;
            }
            let trace = run(lesson.starter, lesson);
            let verdict = lesson
                .check
                .evaluate(&trace, lesson.starter, lesson.max_length);
            assert!(
                !verdict.is_solved(),
                "lesson {index} ({}): the starter already solves it, so there is nothing to do",
                lesson.title
            );
        }
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
        match lesson.check.evaluate(&trace, "+", None) {
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
        match lesson.check.evaluate(&trace, "+[]", None) {
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
        match lesson.check.evaluate(&trace, &brute, lesson.max_length) {
            Verdict::NotYet(why) => assert!(why.contains("limit"), "{why}"),
            other => panic!("expected NotYet, got {other:?}"),
        }
    }
}
