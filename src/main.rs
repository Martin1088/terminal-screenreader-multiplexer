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
use terminal_screenreader_multiplexer::{A11y, AppEvent};

fn main() -> std::io::Result<()> {
    let lines: Vec<String> = (1..=40)
        .map(|i| format!("Zeile {i}: Beispieltext zum Navigieren im Copy-Mode"))
        .collect();

    let (action_tx, action_rx) = channel();
    let mut a11y = A11y::new(action_tx);

    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let mut cursor_line: usize = 0;
    let mut cursor_col: usize = 0;
    let mut top: usize = 0;
    let view_height: usize = 20;

    loop {
        while let Ok(AppEvent::RouteTo { line, grapheme_col }) = action_rx.try_recv() {
            cursor_line = line.min(lines.len().saturating_sub(1));
            cursor_col = grapheme_col;
            if cursor_line < top {
                top = cursor_line;
            } else if cursor_line >= top + view_height {
                top = cursor_line + 1 - view_height;
            }
        }

        execute!(stdout, Clear(ClearType::All))?;
        for (row, line_idx) in (top..(top + view_height).min(lines.len())).enumerate() {
            execute!(stdout, MoveTo(0, row as u16))?;
            write!(stdout, "{}", lines[line_idx])?;
        }

        let cursor_row = (cursor_line - top) as u16;
        execute!(stdout, MoveTo(cursor_col as u16, cursor_row), Show)?;
        stdout.flush()?;

        a11y.on_cursor_moved(&lines, cursor_line, cursor_col);

        if poll(Duration::from_millis(50))?
            && let Event::Key(KeyEvent { code, .. }) = read()?
        {
            match code {
                KeyCode::Down => {
                    if cursor_line + 1 < lines.len() {
                        cursor_line += 1;
                        cursor_col = 0;
                        if cursor_line >= top + view_height {
                            top += 1; // mitscrollen
                        }
                    }
                }
                KeyCode::Up => {
                    if cursor_line > 0 {
                        cursor_line -= 1;
                        cursor_col = 0;
                        if cursor_line < top {
                            top -= 1;
                        }
                    }
                }
                KeyCode::Esc | KeyCode::F(2) => break, // Copy-Mode verlassen
                _ => {}
            }
        }
    }

    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
