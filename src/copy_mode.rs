use crate::{
    classify_line, platform, Key, LineClass, Tones, TONE_BOOKMARK_REMOVED, TONE_BOOKMARK_SET,
    TONE_COPY, TONE_EMPTY, TONE_ERROR, TONE_PREFIX_ARMED, TONE_PREFIX_CANCELLED, TONE_SELECT,
    TONE_WARNING,
};
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;

/// Cursor, scroll and command state of the copy-mode view.
///
/// Commands follow a tmux-style prefix: `F1` arms the prefix, the next key
/// runs as a command (`m`/`n`/`p`, or movement), then the prefix disarms.
/// Without the prefix, the single-letter keys are ignored — in a real
/// multiplexer they would fall through to the shell.
pub struct CopyMode {
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub top: usize,
    pub view_height: usize,
    pub running: bool,
    pub prefix_armed: bool,
    pub bookmarks: BTreeSet<usize>,
    /// Auswahl-Anker `(zeile, graphem-spalte)`; der Fokus ist der Cursor.
    pub selection_anchor: Option<(usize, usize)>,
    pub status: String,
}

impl CopyMode {
    pub fn new(view_height: usize) -> Self {
        Self {
            cursor_line: 0,
            cursor_col: 0,
            top: 0,
            view_height,
            running: true,
            prefix_armed: false,
            bookmarks: BTreeSet::new(),
            selection_anchor: None,
            status: String::new(),
        }
    }

    /// Text zwischen Anker und Cursor, beide Enden einschließlich, Spalten
    /// als Graphem-Indizes (passend zu den Braille-Routing-Positionen).
    pub fn selected_text(&self, lines: &[String]) -> Option<String> {
        let anchor = self.selection_anchor?;
        let cursor = (self.cursor_line, self.cursor_col);
        let ((start_line, start_col), (end_line, end_col)) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };

        fn graphemes_range(line: &str, from: usize, to_inclusive: Option<usize>) -> String {
            let cells: Vec<&str> = line.graphemes(true).collect();
            let from = from.min(cells.len());
            let end = to_inclusive.map_or(cells.len(), |e| (e + 1).min(cells.len()));
            cells[from..end.max(from)].concat()
        }

        if start_line == end_line {
            Some(graphemes_range(&lines[start_line], start_col, Some(end_col)))
        } else {
            let mut out = graphemes_range(&lines[start_line], start_col, None);
            for line in &lines[start_line + 1..end_line] {
                out.push('\n');
                out.push_str(line);
            }
            out.push('\n');
            out.push_str(&graphemes_range(&lines[end_line], 0, Some(end_col)));
            Some(out)
        }
    }

    /// Moves the cursor, scrolls the view along, and sounds the error/
    /// warning tone when landing on a matching line.
    pub fn move_cursor(&mut self, line: usize, col: usize, lines: &[String], tones: &Tones) {
        let line = line.min(lines.len().saturating_sub(1));
        let changed = line != self.cursor_line;
        self.cursor_line = line;
        self.cursor_col = col;
        if self.cursor_line < self.top {
            self.top = self.cursor_line;
        } else if self.cursor_line >= self.top + self.view_height {
            self.top = self.cursor_line + 1 - self.view_height;
        }
        if changed {
            match classify_line(&lines[self.cursor_line]) {
                LineClass::Error => tones.play(TONE_ERROR),
                LineClass::Warning => tones.play(TONE_WARNING),
                LineClass::Plain => {}
            }
        }
    }

    fn toggle_bookmark(&mut self, lines: &[String], tones: &Tones) {
        if self.bookmarks.remove(&self.cursor_line) {
            self.status = format!("Lesezeichen entfernt: {}", lines[self.cursor_line]);
            tones.play(TONE_BOOKMARK_REMOVED);
        } else {
            self.bookmarks.insert(self.cursor_line);
            self.status = format!("Lesezeichen gesetzt: {}", lines[self.cursor_line]);
            tones.play(TONE_BOOKMARK_SET);
        }
    }

    fn jump_bookmark(&mut self, forward: bool, lines: &[String], tones: &Tones) {
        let target = if forward {
            self.bookmarks
                .range(self.cursor_line + 1..)
                .next()
                .or_else(|| self.bookmarks.iter().next())
        } else {
            self.bookmarks
                .range(..self.cursor_line)
                .next_back()
                .or_else(|| self.bookmarks.iter().next_back())
        }
        .copied();
        match target {
            Some(line) => self.move_cursor(line, 0, lines, tones),
            None => {
                self.status = "Keine Lesezeichen".to_string();
                tones.play(TONE_EMPTY);
            }
        }
    }

    fn move_down(&mut self, lines: &[String], tones: &Tones) {
        if self.cursor_line + 1 < lines.len() {
            self.move_cursor(self.cursor_line + 1, 0, lines, tones);
        }
    }

    fn move_up(&mut self, lines: &[String], tones: &Tones) {
        if self.cursor_line > 0 {
            self.move_cursor(self.cursor_line - 1, 0, lines, tones);
        }
    }

    pub fn apply_key(&mut self, key: Key, lines: &[String], tones: &Tones) {
        if self.prefix_armed {
            self.prefix_armed = false;
            match key {
                Key::ToggleBookmark => self.toggle_bookmark(lines, tones),
                Key::NextBookmark => self.jump_bookmark(true, lines, tones),
                Key::PrevBookmark => self.jump_bookmark(false, lines, tones),
                // Bewegung bleibt auch nach dem Präfix Bewegung.
                Key::Down => self.move_down(lines, tones),
                Key::Up => self.move_up(lines, tones),
                // Esc, zweites F1 oder eine Nicht-Befehlstaste bricht den
                // Präfix ab statt etwas anderes auszulösen.
                Key::Exit | Key::Prefix | Key::StartSelection | Key::Copy => {
                    self.status = "Präfix abgebrochen".to_string();
                    tones.play(TONE_PREFIX_CANCELLED);
                }
            }
            return;
        }

        match key {
            Key::Down => self.move_down(lines, tones),
            Key::Up => self.move_up(lines, tones),
            Key::Exit => {
                // Esc hebt zuerst eine aktive Auswahl auf; erst ohne Auswahl
                // verlässt es den Copy-Mode.
                if self.selection_anchor.take().is_some() {
                    self.status = "Auswahl aufgehoben".to_string();
                    tones.play(TONE_PREFIX_CANCELLED);
                } else {
                    self.running = false;
                }
            }
            Key::Prefix => {
                self.prefix_armed = true;
                self.status = "Präfix aktiv".to_string();
                tones.play(TONE_PREFIX_ARMED);
            }
            Key::StartSelection => {
                self.selection_anchor = Some((self.cursor_line, self.cursor_col));
                self.status = "Auswahl gestartet".to_string();
                tones.play(TONE_SELECT);
            }
            Key::Copy => match self.selected_text(lines) {
                Some(text) => {
                    let line_count = text.lines().count().max(1);
                    if platform::set_clipboard(&text) {
                        self.status = if line_count == 1 {
                            "Kopiert: 1 Zeile".to_string()
                        } else {
                            format!("Kopiert: {line_count} Zeilen")
                        };
                        tones.play(TONE_COPY);
                    } else {
                        self.status = "Zwischenablage nicht verfügbar".to_string();
                        tones.play(TONE_EMPTY);
                    }
                    self.selection_anchor = None;
                }
                None => {
                    self.status = "Keine Auswahl".to_string();
                    tones.play(TONE_EMPTY);
                }
            },
            // Ohne Präfix gehören Einzeltasten der Shell, nicht uns.
            Key::ToggleBookmark | Key::NextBookmark | Key::PrevBookmark => {}
        }
    }
}
