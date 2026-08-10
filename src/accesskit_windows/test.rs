use crossterm::{
    cursor::{MoveTo, Show},
    event::{read, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Write};

fn main() -> std::io::Result<()> {
    // Beispiel-Scrollback-Inhalt (später: echter Puffer)
    let lines: Vec<String> = (1..=40)
        .map(|i| format!("Zeile {i}: Beispieltext zum Navigieren im Copy-Mode"))
        .collect();

    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let mut cursor_line: usize = 0; // Cursor-Zeile im Text
    let mut top: usize = 0;          // oberste sichtbare Zeile (Scroll-Offset)
    let view_height: usize = 20;

    loop {
        // Sichtbaren Ausschnitt neu zeichnen
        execute!(stdout, Clear(ClearType::All))?;
        for (row, line_idx) in (top..(top + view_height).min(lines.len())).enumerate() {
            execute!(stdout, MoveTo(0, row as u16))?;
            write!(stdout, "{}", lines[line_idx])?;
        }

        // ENTSCHEIDEND: echten Terminal-Cursor auf die aktive Zeile setzen.
        // Genau das verfolgt der Screen Reader.
        let cursor_row = (cursor_line - top) as u16;
        execute!(stdout, MoveTo(0, cursor_row), Show)?;
        stdout.flush()?;

        // Eingabe
        if let Event::Key(KeyEvent { code, .. }) = read()? {
            match code {
                KeyCode::Down => {
                    if cursor_line + 1 < lines.len() {
                        cursor_line += 1;
                        if cursor_line >= top + view_height {
                            top += 1; // mitscrollen
                        }
                    }
                }
                KeyCode::Up => {
                    if cursor_line > 0 {
                        cursor_line -= 1;
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
