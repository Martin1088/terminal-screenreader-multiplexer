use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineClass {
    Error,
    Warning,
    Plain,
}

// Ganze Wörter, nicht Substrings: "mirror" darf nicht als Fehler gelten.
const ERROR_WORDS: &[&str] = &[
    "error",
    "errors",
    "err",
    "failed",
    "failure",
    "failures",
    "fatal",
    "panic",
    "panicked",
    "exception",
    "exceptions",
    "traceback",
    "denied",
    "refused",
    "abort",
    "aborted",
    "fehler",
    "fehlgeschlagen",
    "abbruch",
];

const WARNING_WORDS: &[&str] = &[
    "warning",
    "warnings",
    "warn",
    "deprecated",
    "caution",
    "warnung",
    "warnungen",
    "achtung",
];

/// Classifies a line by matching whole words (word boundaries,
/// case-insensitive) against known error/warning vocabularies. An error
/// match wins over a warning match anywhere in the line.
///
/// Unicode segmentation alone keeps `error.log` as one word (UAX-29 treats
/// a dot between letters as word-internal, as in "e.g."), so each segment
/// is additionally split at non-alphanumeric characters — matching the
/// `\berror\b`-style boundaries screen-reader tools use, while still never
/// matching substrings like "mirror".
pub fn classify_line(text: &str) -> LineClass {
    let mut warning = false;
    for word in text.unicode_words() {
        for token in word.split(|c: char| !c.is_alphanumeric()) {
            let lower = token.to_lowercase();
            if ERROR_WORDS.contains(&lower.as_str()) {
                return LineClass::Error;
            }
            if WARNING_WORDS.contains(&lower.as_str()) {
                warning = true;
            }
        }
    }
    if warning {
        LineClass::Warning
    } else {
        LineClass::Plain
    }
}
