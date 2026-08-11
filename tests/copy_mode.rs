use terminal_screenreader_multiplexer::{CopyMode, Key, Tones};

fn lines(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("Zeile {i}")).collect()
}

fn mode_and_tones(view_height: usize) -> (CopyMode, Tones) {
    (CopyMode::new(view_height), Tones::new())
}

#[test]
fn starts_at_origin_and_running() {
    let (mode, _tones) = mode_and_tones(20);
    assert_eq!(mode.cursor_line, 0);
    assert_eq!(mode.cursor_col, 0);
    assert_eq!(mode.top, 0);
    assert!(mode.running);
    assert!(!mode.prefix_armed);
    assert!(mode.bookmarks.is_empty());
    assert_eq!(mode.status, "");
}

#[test]
fn down_and_up_move_and_reset_column() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(40);

    mode.apply_key(Key::Down, &lines, &tones);
    assert_eq!(mode.cursor_line, 1);

    mode.move_cursor(1, 7, &lines, &tones);
    assert_eq!(mode.cursor_col, 7);
    mode.apply_key(Key::Down, &lines, &tones);
    assert_eq!((mode.cursor_line, mode.cursor_col), (2, 0));

    mode.apply_key(Key::Up, &lines, &tones);
    assert_eq!(mode.cursor_line, 1);
}

#[test]
fn movement_stops_at_buffer_edges() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(3);

    mode.apply_key(Key::Up, &lines, &tones);
    assert_eq!(mode.cursor_line, 0);

    for _ in 0..10 {
        mode.apply_key(Key::Down, &lines, &tones);
    }
    assert_eq!(mode.cursor_line, 2);
}

#[test]
fn view_scrolls_with_cursor() {
    let (mut mode, tones) = mode_and_tones(5);
    let lines = lines(40);

    for _ in 0..7 {
        mode.apply_key(Key::Down, &lines, &tones);
    }
    // Cursor auf Zeile 7 bei Höhe 5 → oberste sichtbare Zeile ist 3.
    assert_eq!(mode.cursor_line, 7);
    assert_eq!(mode.top, 3);

    mode.move_cursor(0, 0, &lines, &tones);
    assert_eq!(mode.top, 0);
}

#[test]
fn move_cursor_clamps_to_last_line() {
    let (mut mode, tones) = mode_and_tones(5);
    let lines = lines(10);

    // Routing-Ziel hinter dem Pufferende (z. B. veralteter Tree-Stand).
    mode.move_cursor(999, 4, &lines, &tones);
    assert_eq!(mode.cursor_line, 9);
    assert_eq!(mode.cursor_col, 4);
    assert_eq!(mode.top, 5);
}

#[test]
fn exit_stops_the_loop() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);
    mode.apply_key(Key::Exit, &lines, &tones);
    assert!(!mode.running);
}

#[test]
fn command_keys_do_nothing_without_prefix() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    mode.apply_key(Key::NextBookmark, &lines, &tones);
    mode.apply_key(Key::PrevBookmark, &lines, &tones);

    assert!(mode.bookmarks.is_empty());
    assert_eq!(mode.cursor_line, 0);
    assert_eq!(mode.status, "");
}

#[test]
fn prefix_arms_announces_and_disarms_after_one_command() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Prefix, &lines, &tones);
    assert!(mode.prefix_armed);
    assert_eq!(mode.status, "Präfix aktiv");

    mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    assert!(!mode.prefix_armed);
    assert!(mode.bookmarks.contains(&0));

    // Ohne erneutes Präfix ist die Taste wieder wirkungslos.
    mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    assert!(mode.bookmarks.contains(&0));
}

#[test]
fn escape_and_second_prefix_cancel_without_exiting() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::Exit, &lines, &tones);
    assert!(mode.running, "Esc nach Präfix darf Copy-Mode nicht beenden");
    assert!(!mode.prefix_armed);
    assert_eq!(mode.status, "Präfix abgebrochen");

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::Prefix, &lines, &tones);
    assert!(!mode.prefix_armed);
    assert_eq!(mode.status, "Präfix abgebrochen");
}

#[test]
fn movement_after_prefix_still_moves() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::Down, &lines, &tones);
    assert_eq!(mode.cursor_line, 1);
    assert!(!mode.prefix_armed);
}

#[test]
fn bookmark_toggle_labels_with_line_content() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);
    mode.move_cursor(2, 0, &lines, &tones);

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    assert_eq!(mode.status, "Lesezeichen gesetzt: Zeile 3");

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    assert_eq!(mode.status, "Lesezeichen entfernt: Zeile 3");
    assert!(mode.bookmarks.is_empty());
}

#[test]
fn bookmark_jumps_wrap_in_both_directions() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(30);

    for line in [5usize, 10, 20] {
        mode.move_cursor(line, 0, &lines, &tones);
        mode.apply_key(Key::Prefix, &lines, &tones);
        mode.apply_key(Key::ToggleBookmark, &lines, &tones);
    }

    mode.move_cursor(0, 0, &lines, &tones);
    for expected in [5usize, 10, 20, 5] {
        mode.apply_key(Key::Prefix, &lines, &tones);
        mode.apply_key(Key::NextBookmark, &lines, &tones);
        assert_eq!(mode.cursor_line, expected, "vorwärts mit Umbruch");
    }

    for expected in [20usize, 10, 5, 20] {
        mode.apply_key(Key::Prefix, &lines, &tones);
        mode.apply_key(Key::PrevBookmark, &lines, &tones);
        assert_eq!(mode.cursor_line, expected, "rückwärts mit Umbruch");
    }
}

#[test]
fn selection_same_line_inclusive_of_both_ends() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = vec!["Zeile äöü eins".to_string(), "Zeile zwei".to_string()];

    mode.move_cursor(0, 6, &lines, &tones);
    mode.apply_key(Key::StartSelection, &lines, &tones);
    assert_eq!(mode.status, "Auswahl gestartet");
    mode.move_cursor(0, 8, &lines, &tones);

    // Graphem-Spalten 6..=8 sind "äöü", auch mit Mehrbyte-Zeichen.
    assert_eq!(mode.selected_text(&lines).as_deref(), Some("äöü"));
}

#[test]
fn selection_spans_lines_and_direction_does_not_matter() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = vec![
        "erste Zeile".to_string(),
        "mittlere Zeile".to_string(),
        "letzte Zeile".to_string(),
    ];

    // Rückwärts markieren: Anker unten, Cursor wandert nach oben.
    mode.move_cursor(2, 5, &lines, &tones);
    mode.apply_key(Key::StartSelection, &lines, &tones);
    mode.move_cursor(0, 6, &lines, &tones);

    assert_eq!(
        mode.selected_text(&lines).as_deref(),
        Some("Zeile\nmittlere Zeile\nletzte")
    );
}

#[test]
fn escape_cancels_selection_before_exiting() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::StartSelection, &lines, &tones);
    mode.apply_key(Key::Exit, &lines, &tones);
    assert!(mode.running, "Esc mit Auswahl darf Copy-Mode nicht beenden");
    assert!(mode.selection_anchor.is_none());
    assert_eq!(mode.status, "Auswahl aufgehoben");

    mode.apply_key(Key::Exit, &lines, &tones);
    assert!(!mode.running);
}

#[test]
fn copy_without_selection_reports_status() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Copy, &lines, &tones);
    assert_eq!(mode.status, "Keine Auswahl");
}

#[test]
fn copy_clears_anchor_and_reports_line_count() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::StartSelection, &lines, &tones);
    mode.move_cursor(2, 0, &lines, &tones);
    mode.apply_key(Key::Copy, &lines, &tones);

    // Die Stub-Zwischenablage meldet Erfolg, daher der Kopiert-Status.
    assert_eq!(mode.status, "Kopiert: 3 Zeilen");
    assert!(mode.selection_anchor.is_none());
}

#[test]
fn selection_keys_cancel_an_armed_prefix() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::StartSelection, &lines, &tones);
    assert!(!mode.prefix_armed);
    assert!(mode.selection_anchor.is_none());
    assert_eq!(mode.status, "Präfix abgebrochen");
}

#[test]
fn bookmark_jump_without_bookmarks_reports_status() {
    let (mut mode, tones) = mode_and_tones(20);
    let lines = lines(5);

    mode.apply_key(Key::Prefix, &lines, &tones);
    mode.apply_key(Key::NextBookmark, &lines, &tones);
    assert_eq!(mode.cursor_line, 0);
    assert_eq!(mode.status, "Keine Lesezeichen");
}
