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

/// Classifies a line by matching whole words (Unicode word boundaries,
/// case-insensitive) against known error/warning vocabularies. An error
/// match wins over a warning match anywhere in the line.
pub fn classify_line(text: &str) -> LineClass {
    let mut warning = false;
    for word in text.unicode_words() {
        let lower = word.to_lowercase();
        if ERROR_WORDS.contains(&lower.as_str()) {
            return LineClass::Error;
        }
        if WARNING_WORDS.contains(&lower.as_str()) {
            warning = true;
        }
    }
    if warning {
        LineClass::Warning
    } else {
        LineClass::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substrings_do_not_match() {
        assert_eq!(classify_line("mirror the repository"), LineClass::Plain);
        assert_eq!(classify_line("write to stderr"), LineClass::Plain);
        assert_eq!(classify_line("fehlerhaft ist kein Treffer"), LineClass::Plain);
    }

    #[test]
    fn whole_words_match_case_insensitively() {
        assert_eq!(classify_line("Error: file not found"), LineClass::Error);
        assert_eq!(classify_line("build FAILED after 2s"), LineClass::Error);
        assert_eq!(classify_line("Fehler: Verbindung getrennt"), LineClass::Error);
        assert_eq!(classify_line("3 warnings emitted"), LineClass::Warning);
        assert_eq!(classify_line("Warnung: Speicher knapp"), LineClass::Warning);
    }

    #[test]
    fn error_wins_over_warning() {
        assert_eq!(
            classify_line("warning treated as error"),
            LineClass::Error
        );
    }

    #[test]
    fn punctuation_is_a_word_boundary() {
        assert_eq!(classify_line("error[E0308]: mismatched types"), LineClass::Error);
        assert_eq!(classify_line("(error)"), LineClass::Error);
    }
}
