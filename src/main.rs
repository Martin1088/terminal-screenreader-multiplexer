use crossterm::{
    cursor::{MoveTo, Show},
    event::{poll, read, Event, KeyCode, KeyEvent},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::collections::BTreeSet;
use std::io::{stdout, Write};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use terminal_screenreader_multiplexer::{
    classify_line, A11y, AppEvent, Key, LineClass, Tones, TONE_ACTIVITY, TONE_BOOKMARK_REMOVED,
    TONE_BOOKMARK_SET, TONE_ERROR, TONE_LAYER_OFF, TONE_LAYER_ON, TONE_NO_BOOKMARKS, TONE_WARNING,
};

const MAX_LINES: usize = 200;
const OUTPUT_EVERY: Duration = Duration::from_millis(2500);

struct CopyMode {
    cursor_line: usize,
    cursor_col: usize,
    top: usize,
    view_height: usize,
    running: bool,
    layer: bool,
    bookmarks: BTreeSet<usize>,
    status: String,
}

impl CopyMode {
    /// Moves the cursor, scrolls the view along, and sounds the error/
    /// warning tone when landing on a matching line.
    fn move_cursor(&mut self, line: usize, col: usize, lines: &[String], tones: &Tones) {
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

    fn apply_key(&mut self, key: Key, lines: &[String], tones: &Tones) {
        match key {
            Key::Down => {
                if self.cursor_line + 1 < lines.len() {
                    self.move_cursor(self.cursor_line + 1, 0, lines, tones);
                }
            }
            Key::Up => {
                if self.cursor_line > 0 {
                    self.move_cursor(self.cursor_line - 1, 0, lines, tones);
                }
            }
            Key::Exit => self.running = false, // Copy-Mode verlassen
            Key::ToggleLayer => {
                self.layer = !self.layer;
                if self.layer {
                    self.status = "Befehlsebene ein".to_string();
                    tones.play(TONE_LAYER_ON);
                } else {
                    self.status = "Befehlsebene aus".to_string();
                    tones.play(TONE_LAYER_OFF);
                }
            }
            // Einzeltasten-Befehle gelten nur bei aktiver Befehlsebene; sonst
            // würden sie in einem echten Multiplexer an die Shell gehen.
            Key::ToggleBookmark if self.layer => self.toggle_bookmark(lines, tones),
            Key::NextBookmark if self.layer => self.jump_bookmark(true, lines, tones),
            Key::PrevBookmark if self.layer => self.jump_bookmark(false, lines, tones),
            Key::ToggleBookmark | Key::NextBookmark | Key::PrevBookmark => {}
        }
    }
}

fn activity_debounce() -> Duration {
    let ms = std::env::var("TSM_ACTIVITY_DEBOUNCE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1000)
        .clamp(100, 10_000);
    Duration::from_millis(ms)
}

fn main() -> std::io::Result<()> {
    let mut lines: Vec<String> = (1..=40)
        .map(|i| format!("Zeile {i}: Beispieltext zum Navigieren im Copy-Mode"))
        .collect();

    let (action_tx, action_rx) = channel();
    let mut a11y = A11y::new(action_tx);
    let tones = Tones::new();
    let debounce = activity_debounce();

    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let mut mode = CopyMode {
        cursor_line: 0,
        cursor_col: 0,
        top: 0,
        view_height: 20,
        running: true,
        layer: false,
        bookmarks: BTreeSet::new(),
        status: String::new(),
    };

    let mut last_output = Instant::now();
    let mut last_activity_tone: Option<Instant> = None;

    while mode.running {
        // Simulierte Hintergrund-Ausgabe, damit Activity-Töne und
        // Fehler-/Warnungserkennung etwas zum Anzeigen haben.
        if lines.len() < MAX_LINES && last_output.elapsed() >= OUTPUT_EVERY {
            last_output = Instant::now();
            let n = lines.len() + 1;
            let text = match n % 5 {
                0 => format!("Zeile {n}: Fehler: Demo-Verbindung fehlgeschlagen"),
                3 => format!("Zeile {n}: Warnung: Demo-Speicher wird knapp"),
                _ => format!("Zeile {n}: neue Ausgabe im Hintergrund"),
            };
            lines.push(text);
            let due = last_activity_tone.is_none_or(|t| t.elapsed() >= debounce);
            if due {
                tones.play(TONE_ACTIVITY);
                last_activity_tone = Some(Instant::now());
            }
        }

        while let Ok(event) = action_rx.try_recv() {
            match event {
                AppEvent::RouteTo { line, grapheme_col } => {
                    mode.move_cursor(line, grapheme_col, &lines, &tones);
                }
                AppEvent::Key(key) => mode.apply_key(key, &lines, &tones),
            }
        }

        execute!(stdout, Clear(ClearType::All))?;
        for (row, line_idx) in (mode.top..(mode.top + mode.view_height).min(lines.len())).enumerate()
        {
            execute!(stdout, MoveTo(0, row as u16))?;
            write!(stdout, "{}", lines[line_idx])?;
        }

        let cursor_row = (mode.cursor_line - mode.top) as u16;
        execute!(stdout, MoveTo(mode.cursor_col as u16, cursor_row), Show)?;
        stdout.flush()?;

        a11y.update(&lines, mode.cursor_line, mode.cursor_col, &mode.status);

        // Terminal input still works when the console has focus (and is the
        // only path on non-Windows); with the bridge window focused, keys
        // arrive as AppEvent::Key above instead.
        if poll(Duration::from_millis(50))?
            && let Event::Key(KeyEvent { code, .. }) = read()?
        {
            let key = match code {
                KeyCode::Down => Some(Key::Down),
                KeyCode::Up => Some(Key::Up),
                KeyCode::Esc | KeyCode::F(2) => Some(Key::Exit),
                KeyCode::F(1) => Some(Key::ToggleLayer),
                KeyCode::Char('m' | 'M') => Some(Key::ToggleBookmark),
                KeyCode::Char('n' | 'N') => Some(Key::NextBookmark),
                KeyCode::Char('p' | 'P') => Some(Key::PrevBookmark),
                _ => None,
            };
            if let Some(key) = key {
                mode.apply_key(key, &lines, &tones);
            }
        }
    }

    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
