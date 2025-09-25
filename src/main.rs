use crossterm::{
    ExecutableCommand,
    terminal::{enable_raw_mode, disable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
    cursor,
    event::{read, Event, KeyEvent, KeyCode, KeyEventKind, KeyModifiers},
};
use ropey::Rope;
use std::io::{stdout, Write};
use std::fs::{write, read_to_string};
use std::io;
use std::time::{Instant, Duration};

fn save_file(path: &str, buffer: &Rope) -> io::Result<()> {
    write(path, buffer.slice(..).to_string())
}

fn open_file(path: &str) -> io::Result<Rope> {
    let content = read_to_string(path)?;
    Ok(Rope::from_str(&content))
}

enum InputMode {
    Editing,
    EnteringFileNameOpen,
    EnteringFileNameSave,
}

fn main() -> io::Result<()> {
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut buffer = Rope::new();
    let mut cursor_char_idx = 0; // Cursor as char index in rope

    let mut status_message: Option<String> = None;
    let mut status_message_time: Option<Instant> = None;

    let mut input_mode = InputMode::Editing;
    let mut filename_input = String::new();
    let mut current_file: Option<String> = None;

    // Welcome message at top
    stdout.execute(Clear(ClearType::All))?;
    stdout.execute(cursor::MoveTo(0, 0))?;
    writeln!(stdout, "Welcome to Rust Editor!")?;
    stdout.flush()?;

    'mainloop: loop {
        let (cols, rows) = size()?;

        stdout.execute(cursor::Hide)?;

        stdout.execute(cursor::MoveTo(0, 1))?;
        stdout.execute(Clear(ClearType::FromCursorDown))?;

        stdout.execute(cursor::MoveTo(0, rows - 1))?;
        stdout.execute(Clear(ClearType::CurrentLine))?;

        // Draw status or prompt line
        match input_mode {
            InputMode::EnteringFileNameOpen =>
                write!(stdout, "Open file: {}", filename_input)?,
            InputMode::EnteringFileNameSave =>
                write!(stdout, "Save file: {}", filename_input)?,
            InputMode::Editing => {
                if let Some(msg) = &status_message {
                    if let Some(start) = status_message_time {
                        if start.elapsed() < Duration::from_secs(3) {
                            write!(stdout, "{}", msg)?;
                        } else {
                            status_message = None;
                            status_message_time = None;
                        }
                    }
                }
            }
        }

        // Calculate current line and column in rope
        let current_line = buffer.char_to_line(cursor_char_idx);
        let line_start_char_idx = buffer.line_to_char(current_line);
        let cursor_col = cursor_char_idx - line_start_char_idx;

        let max_lines = (rows - 2) as usize;
        let total_lines = buffer.len_lines();

        for i in 0..max_lines {
            if i >= total_lines { break; }
            stdout.execute(cursor::MoveTo(0, (i + 1) as u16))?;
            let line = buffer.line(i);
            // Exclude trailing newline if exists
            let line_str = if line.len_chars() > 0 && line.char(line.len_chars() - 1) == '\n' {
                line.slice(0..line.len_chars() - 1)
            } else {
                line
            };
            write!(stdout, "{}", line_str)?;
        }

        // Position cursor (adjusted for line offset)
        if (current_line as u16) < rows - 2 {
            stdout.execute(cursor::MoveTo(cursor_col as u16, (current_line + 1) as u16))?;
        }

        stdout.execute(cursor::Show)?;
        stdout.flush()?;

        if let Event::Key(KeyEvent { code, kind, modifiers, .. }) = read()? {
            if kind == KeyEventKind::Press {
                match input_mode {
                    InputMode::Editing => {
                        match code {
                            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) =>
                                break 'mainloop,
                            KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(filename) = &current_file {
                                    match save_file(filename, &buffer) {
                                        Ok(_) => {
                                            status_message = Some(format!("File saved to {}", filename));
                                            status_message_time = Some(Instant::now());
                                        },
                                        Err(e) => {
                                            status_message = Some(format!("Error saving file: {}", e));
                                            status_message_time = Some(Instant::now());
                                        }
                                    }
                                } else {
                                    input_mode = InputMode::EnteringFileNameSave;
                                    filename_input.clear();
                                }
                            },
                            KeyCode::Char('o') if modifiers.contains(KeyModifiers::CONTROL) => {
                                input_mode = InputMode::EnteringFileNameOpen;
                                filename_input.clear();
                            },
                            KeyCode::Char(c) => {
                                buffer.insert_char(cursor_char_idx, c);
                                cursor_char_idx += 1;
                            },
                            KeyCode::Left => {
                                if cursor_char_idx > 0 {
                                    cursor_char_idx -= 1;
                                }
                            },
                            KeyCode::Right => {
                                if cursor_char_idx < buffer.len_chars() {
                                    cursor_char_idx += 1;
                                }
                            },
                            KeyCode::Up => {
                                if current_line > 0 {
                                    let target_line = current_line - 1;
                                    let target_line_start = buffer.line_to_char(target_line);
                                    let target_line_len = buffer.line(target_line).len_chars();
                                    let new_col = cursor_col.min(target_line_len.saturating_sub(1));
                                    cursor_char_idx = target_line_start + new_col;
                                }
                            },
                            KeyCode::Down => {
                                if current_line + 1 < total_lines {
                                    let target_line = current_line + 1;
                                    let target_line_start = buffer.line_to_char(target_line);
                                    let target_line_len = buffer.line(target_line).len_chars();
                                    let new_col = cursor_col.min(target_line_len.saturating_sub(1));
                                    cursor_char_idx = target_line_start + new_col;
                                }
                            },
                            KeyCode::Backspace => {
                                if cursor_char_idx > 0 {
                                    buffer.remove(cursor_char_idx - 1..cursor_char_idx);
                                    cursor_char_idx -= 1;
                                }
                            },
                            KeyCode::Enter => {
                                buffer.insert_char(cursor_char_idx, '\n');
                                cursor_char_idx += 1;
                            },
                            _ => {}
                        }
                    },
                    InputMode::EnteringFileNameOpen => {
                        match code {
                            KeyCode::Esc => {
                                input_mode = InputMode::Editing;
                                status_message = Some("Open cancelled".to_string());
                                status_message_time = Some(Instant::now());
                            },
                            KeyCode::Enter => {
                                match open_file(&filename_input) {
                                    Ok(new_buffer) => {
                                        buffer = new_buffer;
                                        cursor_char_idx = 0;
                                        current_file = Some(filename_input.clone());
                                        status_message = Some(format!("File loaded from {}", filename_input));
                                    },
                                    Err(e) => {
                                        status_message = Some(format!("Error opening file: {}", e));
                                    }
                                }
                                status_message_time = Some(Instant::now());
                                input_mode = InputMode::Editing;
                            },
                            KeyCode::Backspace => { filename_input.pop(); },
                            KeyCode::Char(c) => { filename_input.push(c); },
                            _ => {}
                        }
                    },
                    InputMode::EnteringFileNameSave => {
                        match code {
                            KeyCode::Esc => {
                                input_mode = InputMode::Editing;
                                status_message = Some("Save cancelled".to_string());
                                status_message_time = Some(Instant::now());
                            },
                            KeyCode::Enter => {
                                match save_file(&filename_input, &buffer) {
                                    Ok(_) => {
                                        current_file = Some(filename_input.clone());
                                        status_message = Some(format!("File saved to {}", filename_input));
                                    },
                                    Err(e) => {
                                        status_message = Some(format!("Error saving file: {}", e));
                                    }
                                }
                                status_message_time = Some(Instant::now());
                                input_mode = InputMode::Editing;
                            },
                            KeyCode::Backspace => { filename_input.pop(); },
                            KeyCode::Char(c) => { filename_input.push(c); },
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;
    Ok(())
}
