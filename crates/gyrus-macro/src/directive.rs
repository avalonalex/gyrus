//! The directives the expander knows about, in one place.
//!
//! This vocabulary was spelled out in five: an enum covering two of them, the
//! dispatch match, a `PLANNED` list, a sentence naming them in error hints,
//! and a prefix check for whether a line declares a name. They had already
//! drifted once -- two hints went on advertising `@define` alone after `@var`
//! and `@to` shipped, so a mistyped `@too` was told `@to` did not exist -- and
//! adding `@macro` would have meant finding all five by hand.
//!
//! Now a new directive is one variant and a compile error in every `match`
//! until it is answered.

/// Everything spelled `@something`, built or planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Directive {
    Define,
    Var,
    To,
    Here,
    Macro,
    Include,
    Ifdef,
    Ifndef,
    Endif,
}

impl Directive {
    pub(crate) const ALL: [Directive; 9] = [
        Directive::Define,
        Directive::Var,
        Directive::To,
        Directive::Here,
        Directive::Macro,
        Directive::Include,
        Directive::Ifdef,
        Directive::Ifndef,
        Directive::Endif,
    ];

    pub(crate) fn spelling(self) -> &'static str {
        match self {
            Directive::Define => "define",
            Directive::Var => "var",
            Directive::To => "to",
            Directive::Here => "here",
            Directive::Macro => "macro",
            Directive::Include => "include",
            Directive::Ifdef => "ifdef",
            Directive::Ifndef => "ifndef",
            Directive::Endif => "endif",
        }
    }

    pub(crate) fn from_spelling(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.spelling() == name)
    }

    /// Whether the expander implements it. The rest are refused by name
    /// rather than called unknown, which would be a lie about why they failed.
    pub(crate) fn implemented(self) -> bool {
        self.declaration().is_some()
            || matches!(self, Directive::To | Directive::Here | Directive::Macro)
    }

    /// Whether it can emit instructions nobody wrote literally. Only `@to`
    /// does today.
    ///
    /// `#[cfg(test)]` because the origin-map invariant is its only reader:
    /// the check used to be a `"@to"` literal in the test, which was the
    /// directive vocabulary written down a sixth time. When `@macro` emits,
    /// this loses the attribute rather than gaining a second copy.
    #[cfg(test)]
    pub(crate) fn emits(self) -> bool {
        matches!(self, Directive::To)
    }

    /// How it declares a name, if it declares one.
    pub(crate) fn declaration(self) -> Option<Declaration> {
        match self {
            Directive::Define => Some(Declaration::Define),
            Directive::Var => Some(Declaration::Var),
            _ => None,
        }
    }
}

/// A directive that introduces a name, and the examples its errors need.
///
/// Separate from [`Directive`] so that the example text is total over its own
/// domain: every declaring directive has a name example and a value example,
/// and `@to` has neither. One enum covering both would need an `Option` and an
/// `expect` at every use, which is the fallback this module exists to remove.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Declaration {
    Define,
    Var,
}

impl Declaration {
    pub(crate) fn directive(self) -> Directive {
        match self {
            Declaration::Define => Directive::Define,
            Declaration::Var => Directive::Var,
        }
    }

    /// What to write where the name goes.
    pub(crate) fn name_example(self) -> &'static str {
        match self {
            Declaration::Define => "`@define CHAR_A 65`",
            Declaration::Var => "`@var counter at 0`",
        }
    }

    /// The whole sentence for a missing value, so the message is readable in
    /// one place rather than assembled from a noun and an example.
    pub(crate) fn missing_value(self, name: &str) -> String {
        match self {
            Declaration::Define => {
                format!("expected a value for '{name}', as in `@define {name} 65`")
            }
            Declaration::Var => format!("expected a cell for '{name}', as in `@var {name} at 0`"),
        }
    }
}

/// The sentence hints use to say what is understood, generated rather than
/// typed. Cold path only, so building a `String` costs nothing that matters.
pub(crate) fn understood() -> String {
    let names: Vec<String> = Directive::ALL
        .into_iter()
        .filter(|d| d.implemented())
        .map(|d| format!("@{}", d.spelling()))
        .collect();
    let (last, rest) = names.split_last().expect("some directive is implemented");
    format!(
        "The expander understands {} and {last}, plus repeat counts like +{{N}}.",
        rest.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_round_trips() {
        for directive in Directive::ALL {
            assert_eq!(
                Directive::from_spelling(directive.spelling()),
                Some(directive)
            );
        }
        assert_eq!(Directive::from_spelling("wibble"), None);
    }

    #[test]
    fn the_understood_sentence_names_what_is_implemented_and_nothing_else() {
        let sentence = understood();
        for directive in Directive::ALL {
            let named = sentence.contains(&format!("@{}", directive.spelling()));
            assert_eq!(
                named,
                directive.implemented(),
                "@{} is {}named but {}implemented",
                directive.spelling(),
                if named { "" } else { "not " },
                if directive.implemented() { "" } else { "not " }
            );
        }
    }
}
