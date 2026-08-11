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
use std::time::{Duration, Instant};
use terminal_screenreader_multiplexer::{A11y, AppEvent, CopyMode, Key, Tones, TONE_ACTIVITY};

const MAX_LINES: usize = 200;
const OUTPUT_EVERY: Duration = Duration::from_millis(2500);

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

    let mut mode = CopyMode::new(20);
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

        a11y.update(
            &lines,
            mode.cursor_line,
            mode.cursor_col,
            mode.selection_anchor,
            &mode.status,
        );

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
                KeyCode::F(1) => Some(Key::Prefix),
                KeyCode::Char(' ') => Some(Key::StartSelection),
                KeyCode::Enter => Some(Key::Copy),
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
