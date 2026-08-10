use crate::{
    classify_line, Key, LineClass, Tones, TONE_BOOKMARK_REMOVED, TONE_BOOKMARK_SET, TONE_ERROR,
    TONE_NO_BOOKMARKS, TONE_PREFIX_ARMED, TONE_PREFIX_CANCELLED, TONE_WARNING,
};
use std::collections::BTreeSet;

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
            status: String::new(),
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
                tones.play(TONE_NO_BOOKMARKS);
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
                // Esc oder zweites F1 bricht den Präfix ab statt zu beenden.
                Key::Exit | Key::Prefix => {
                    self.status = "Präfix abgebrochen".to_string();
                    tones.play(TONE_PREFIX_CANCELLED);
                }
            }
            return;
        }

        match key {
            Key::Down => self.move_down(lines, tones),
            Key::Up => self.move_up(lines, tones),
            Key::Exit => self.running = false, // Copy-Mode verlassen
            Key::Prefix => {
                self.prefix_armed = true;
                self.status = "Präfix aktiv".to_string();
                tones.play(TONE_PREFIX_ARMED);
            }
            // Ohne Präfix gehören Einzeltasten der Shell, nicht uns.
            Key::ToggleBookmark | Key::NextBookmark | Key::PrevBookmark => {}
        }
    }
}
