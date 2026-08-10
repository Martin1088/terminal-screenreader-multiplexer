use terminal_screenreader_multiplexer::{classify_line, LineClass};

#[test]
fn substrings_do_not_match() {
    assert_eq!(classify_line("mirror the repository"), LineClass::Plain);
    assert_eq!(classify_line("write to stderr"), LineClass::Plain);
    assert_eq!(classify_line("fehlerhaft ist kein Treffer"), LineClass::Plain);
    assert_eq!(classify_line("they warned us"), LineClass::Plain);
    assert_eq!(classify_line("terrors of the deep"), LineClass::Plain);
}

#[test]
fn whole_words_match_case_insensitively() {
    assert_eq!(classify_line("Error: file not found"), LineClass::Error);
    assert_eq!(classify_line("build FAILED after 2s"), LineClass::Error);
    assert_eq!(classify_line("thread 'main' PANICKED"), LineClass::Error);
    assert_eq!(classify_line("Fehler: Verbindung getrennt"), LineClass::Error);
    assert_eq!(classify_line("Zugriff verweigert, Abbruch"), LineClass::Error);
    assert_eq!(classify_line("3 warnings emitted"), LineClass::Warning);
    assert_eq!(classify_line("WARN slow query"), LineClass::Warning);
    assert_eq!(classify_line("Warnung: Speicher knapp"), LineClass::Warning);
    assert_eq!(classify_line("use of deprecated function"), LineClass::Warning);
}

#[test]
fn error_wins_over_warning() {
    assert_eq!(classify_line("warning treated as error"), LineClass::Error);
    assert_eq!(classify_line("error before, warning after"), LineClass::Error);
    assert_eq!(classify_line("warning first, then failed"), LineClass::Error);
}

#[test]
fn punctuation_is_a_word_boundary() {
    assert_eq!(
        classify_line("error[E0308]: mismatched types"),
        LineClass::Error
    );
    assert_eq!(classify_line("(error)"), LineClass::Error);
    assert_eq!(classify_line("tail -f /var/log/error.log"), LineClass::Error);
    assert_eq!(classify_line("panic! at line 3"), LineClass::Error);
}

#[test]
fn empty_and_plain_lines() {
    assert_eq!(classify_line(""), LineClass::Plain);
    assert_eq!(classify_line("   "), LineClass::Plain);
    assert_eq!(
        classify_line("Zeile 7: Beispieltext zum Navigieren im Copy-Mode"),
        LineClass::Plain
    );
}
