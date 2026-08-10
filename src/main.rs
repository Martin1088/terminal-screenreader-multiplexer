use crossterm::{
    cursor::{MoveTo, Show},
    event::{poll, read, Event, KeyCode, KeyEvent},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::io::{stdout, Write};
use std::sync::mpsc::channel;
use std::time::Duration;
use terminal_screenreader_multiplexer::{A11y, AppEvent, Key};

struct CopyMode {
    cursor_line: usize,
    cursor_col: usize,
    top: usize,
    view_height: usize,
    running: bool,
}

impl CopyMode {
    fn route_to(&mut self, line: usize, grapheme_col: usize, line_count: usize) {
        self.cursor_line = line.min(line_count.saturating_sub(1));
        self.cursor_col = grapheme_col;
        if self.cursor_line < self.top {
            self.top = self.cursor_line;
        } else if self.cursor_line >= self.top + self.view_height {
            self.top = self.cursor_line + 1 - self.view_height;
        }
    }

    fn apply_key(&mut self, key: Key, line_count: usize) {
        match key {
            Key::Down => {
                if self.cursor_line + 1 < line_count {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                    if self.cursor_line >= self.top + self.view_height {
                        self.top += 1; // mitscrollen
                    }
                }
            }
            Key::Up => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = 0;
                    if self.cursor_line < self.top {
                        self.top -= 1;
                    }
                }
            }
            Key::Exit => self.running = false, // Copy-Mode verlassen
        }
    }
}

fn main() -> std::io::Result<()> {
    let lines: Vec<String> = (1..=40)
        .map(|i| format!("Zeile {i}: Beispieltext zum Navigieren im Copy-Mode"))
        .collect();

    let (action_tx, action_rx) = channel();
    let mut a11y = A11y::new(action_tx);

    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let mut mode = CopyMode {
        cursor_line: 0,
        cursor_col: 0,
        top: 0,
        view_height: 20,
        running: true,
    };

    while mode.running {
        while let Ok(event) = action_rx.try_recv() {
            match event {
                AppEvent::RouteTo { line, grapheme_col } => {
                    mode.route_to(line, grapheme_col, lines.len());
                }
                AppEvent::Key(key) => mode.apply_key(key, lines.len()),
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

        a11y.on_cursor_moved(&lines, mode.cursor_line, mode.cursor_col);

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
                _ => None,
            };
            if let Some(key) = key {
                mode.apply_key(key, lines.len());
            }
        }
    }

    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
