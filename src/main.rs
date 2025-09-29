use crossterm::{
    ExecutableCommand,
    terminal::{enable_raw_mode, disable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
    cursor,
    event::{read, Event, KeyEvent, KeyCode, KeyEventKind, KeyModifiers},
};
use ropey::Rope;
use std::collections::HashSet;
use std::fs::{write, read_to_string};
use std::io::{self, stdout, Write};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
enum EditOp {
    Insert { char_idx: usize, content: String },
    Delete { char_idx: usize, content: String },
}

#[derive(Clone, Debug)]
struct EditAction {
    ops: Vec<EditOp>,
    timestamp: Instant,
}

const GROUP_TIME_THRESHOLD: Duration = Duration::from_millis(200);
const GUTTER_WIDTH: usize = 4; // compact gutter width

fn save_file(path: &str, buffer: &Rope) -> io::Result<()> {
    write(path, buffer.slice(..).to_string())
}

fn open_file(path: &str) -> io::Result<Rope> {
    let content = read_to_string(path)?;
    Ok(Rope::from_str(&content))
}

fn safe_remove(buffer: &mut Rope, start: usize, length: usize) {
    let end = (start + length).min(buffer.len_chars());
    if start < end {
        buffer.remove(start..end);
    }
}

fn safe_insert(buffer: &mut Rope, idx: usize, content: &str) -> usize {
    let safe_idx = idx.min(buffer.len_chars());
    buffer.insert(safe_idx, content);
    safe_idx + content.len()
}

fn add_edit_op(undo_stack: &mut Vec<EditAction>, op: EditOp) {
    let now = Instant::now();
    if let Some(last_action) = undo_stack.last_mut() {
        if now.duration_since(last_action.timestamp) < GROUP_TIME_THRESHOLD {
            last_action.ops.push(op);
            last_action.timestamp = now;
            return;
        }
    }
    undo_stack.push(EditAction { ops: vec![op], timestamp: now });
}

fn undo_action(buffer: &mut Rope, cursor_char_idx: &mut usize,
               undo_stack: &mut Vec<EditAction>, redo_stack: &mut Vec<EditAction>,
               dirty_lines: &mut HashSet<usize>) {
    if let Some(action) = undo_stack.pop() {
        for op in action.ops.iter().rev() {
            match op.clone() {
                EditOp::Insert { char_idx, content } => {
                    safe_remove(buffer, char_idx, content.len());
                    *cursor_char_idx = char_idx;
                    dirty_lines.insert(buffer.char_to_line(char_idx));
                }
                EditOp::Delete { char_idx, content } => {
                    *cursor_char_idx = safe_insert(buffer, char_idx, &content);
                    dirty_lines.insert(buffer.char_to_line(char_idx));
                }
            }
        }
        redo_stack.push(action);
    }
}

fn redo_action(buffer: &mut Rope, cursor_char_idx: &mut usize,
               undo_stack: &mut Vec<EditAction>, redo_stack: &mut Vec<EditAction>,
               dirty_lines: &mut HashSet<usize>) {
    if let Some(action) = redo_stack.pop() {
        for op in &action.ops {
            match op.clone() {
                EditOp::Insert { char_idx, content } => {
                    *cursor_char_idx = safe_insert(buffer, char_idx, &content);
                    dirty_lines.insert(buffer.char_to_line(char_idx));
                }
                EditOp::Delete { char_idx, content } => {
                    safe_remove(buffer, char_idx, content.len());
                    *cursor_char_idx = char_idx.min(buffer.len_chars());
                    dirty_lines.insert(buffer.char_to_line(char_idx));
                }
            }
        }
        undo_stack.push(action);
    }
}

struct VirtualScreen {
    lines: Vec<String>,
    cursor_pos: (u16, u16),
}

impl VirtualScreen {
    fn new(rows: usize) -> Self {
        VirtualScreen {
            lines: vec!["".to_string(); rows],
            cursor_pos: (0, 0),
        }
    }

    fn update_line(&mut self, index: usize, content: &str) {
        if index < self.lines.len() {
            self.lines[index] = content.to_string();
        }
    }

    fn get_line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|s| s.as_str())
    }

    fn set_cursor(&mut self, x: u16, y: u16) {
        self.cursor_pos = (x, y);
    }
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
    let mut cursor_char_idx = 0;
    let mut viewport_row = 0;

    let mut status_message: Option<String> = None;
    let mut status_message_time: Option<Instant> = None;

    let mut input_mode = InputMode::Editing;
    let mut filename_input = String::new();
    let mut current_file: Option<String> = None;

    let mut dirty_lines: HashSet<usize> = HashSet::new();

    let mut undo_stack: Vec<EditAction> = Vec::new();
    let mut redo_stack: Vec<EditAction> = Vec::new();

    let mut clipboard = String::new();

    let (cols, rows) = size()?;
    let max_lines = (rows - 2) as usize;

    let mut virtual_screen = VirtualScreen::new(max_lines);

    for i in 0..max_lines {
        dirty_lines.insert(i);
    }

    let mut cursor_visible = true;
    let mut last_cursor_toggle = Instant::now();
    const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

    'mainloop: loop {
        if last_cursor_toggle.elapsed() >= CURSOR_BLINK_INTERVAL {
            cursor_visible = !cursor_visible;
            last_cursor_toggle = Instant::now();
        }

        let total_lines = buffer.len_lines();

        stdout.execute(cursor::Hide)?;

        stdout.execute(cursor::MoveTo(0, 0))?;
        stdout.execute(Clear(ClearType::CurrentLine))?;
        writeln!(stdout, "Welcome to rusty")?;

        stdout.execute(cursor::MoveTo(0, rows - 1))?;
        stdout.execute(Clear(ClearType::CurrentLine))?;
        match input_mode {
            InputMode::EnteringFileNameOpen => write!(stdout, "Open file: {}", filename_input)?,
            InputMode::EnteringFileNameSave => write!(stdout, "Save file: {}", filename_input)?,
            InputMode::Editing => {
                if let Some(msg) = &status_message {
                    if let Some(start) = status_message_time {
                        if start.elapsed() < Duration::from_secs(3) {
                            write!(stdout, "{}", msg)?;
                        }
                    }
                }
            }
        }

        let current_line = buffer.char_to_line(cursor_char_idx);
        let line_start_char_idx = buffer.line_to_char(current_line);
        let cursor_col = cursor_char_idx.saturating_sub(line_start_char_idx);

        let prev_viewport_row = viewport_row;
        if current_line < viewport_row {
            viewport_row = current_line;
            for i in viewport_row..prev_viewport_row {
                dirty_lines.insert(i);
            }
        } else if current_line >= viewport_row + max_lines {
            viewport_row = current_line - max_lines + 1;
            for i in (prev_viewport_row + max_lines)..(viewport_row + max_lines) {
                dirty_lines.insert(i);
            }
        }

        // First, redraw all dirty lines corresponding to buffer lines
        for &line_idx in &dirty_lines {
            if line_idx < viewport_row || line_idx >= viewport_row + max_lines || line_idx >= total_lines {
                continue;
            }
            let view_line_idx = line_idx - viewport_row;

            let rope_line = buffer.line(line_idx);
            let line_str = if rope_line.len_chars() > 0 && rope_line.char(rope_line.len_chars() - 1) == '\n' {
                rope_line.slice(0..rope_line.len_chars() - 1).to_string()
            } else {
                rope_line.to_string()
            };

            let line_number_str = format!("{:>width$}", line_idx + 1, width = GUTTER_WIDTH);
            let combined_line = format!("{} {}", line_number_str, line_str);

            let cached_line = virtual_screen.get_line(view_line_idx).unwrap_or("");
            if cached_line != combined_line {
                stdout.execute(cursor::MoveTo(0, (view_line_idx + 1) as u16))?;
                stdout.execute(Clear(ClearType::CurrentLine))?;
                write!(stdout, "{}", combined_line)?;
                virtual_screen.update_line(view_line_idx, &combined_line);
            }
        }
        dirty_lines.clear();

        // Second, draw "~" lines for any remaining lines without text, inside viewport
        for view_line_idx in 0..max_lines {
            let real_line_idx = viewport_row + view_line_idx;
            if real_line_idx < total_lines {
                continue; // skip lines with content handled above
            }
            let tilde_line = format!("{:>width$}~ ", "", width = GUTTER_WIDTH - 1);
            let cached_line = virtual_screen.get_line(view_line_idx).unwrap_or("");
            if cached_line != tilde_line {
                stdout.execute(cursor::MoveTo(0, (view_line_idx + 1) as u16))?;
                stdout.execute(Clear(ClearType::CurrentLine))?;
                write!(stdout, "{}", tilde_line)?;
                virtual_screen.update_line(view_line_idx, &tilde_line);
            }
        }

        let cursor_y = (current_line.saturating_sub(viewport_row) + 1) as u16;
        let cursor_x = cursor_col as u16 + (GUTTER_WIDTH as u16) + 1;
        stdout.execute(cursor::MoveTo(cursor_x, cursor_y))?;

        if cursor_visible {
            stdout.execute(cursor::Show)?;
        } else {
            stdout.execute(cursor::Hide)?;
        }

        stdout.flush()?;

        if let Event::Key(KeyEvent { code, kind, modifiers, .. }) = read()? {
            if kind == KeyEventKind::Press {
                cursor_visible = true;
                last_cursor_toggle = Instant::now();

                match input_mode {
                    InputMode::Editing => match code {
                        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => break 'mainloop,

                        KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(filename) = &current_file {
                                if let Err(e) = save_file(filename, &buffer) {
                                    status_message = Some(format!("Error saving file: {}", e));
                                    status_message_time = Some(Instant::now());
                                } else {
                                    status_message = Some(format!("File saved to {}", filename));
                                    status_message_time = Some(Instant::now());
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

                        KeyCode::Char('z') if modifiers.contains(KeyModifiers::CONTROL) => {
                            undo_action(&mut buffer, &mut cursor_char_idx, &mut undo_stack, &mut redo_stack, &mut dirty_lines);
                        },

                        KeyCode::Char('y') if modifiers.contains(KeyModifiers::CONTROL) => {
                            redo_action(&mut buffer, &mut cursor_char_idx, &mut undo_stack, &mut redo_stack, &mut dirty_lines);
                        },

                        KeyCode::Char('x') if modifiers.contains(KeyModifiers::CONTROL) => {
                            let current_line = buffer.char_to_line(cursor_char_idx);
                            let line_start = buffer.line_to_char(current_line);
                            let line_end = if current_line + 1 < total_lines {
                                buffer.line_to_char(current_line + 1)
                            } else {
                                buffer.len_chars()
                            };
                            let content = buffer.slice(line_start..line_end).to_string();
                            clipboard = content.clone();
                            safe_remove(&mut buffer, line_start, content.len());
                            cursor_char_idx = line_start;
                            add_edit_op(&mut undo_stack, EditOp::Delete { char_idx: line_start, content });
                            redo_stack.clear();
                            dirty_lines.insert(current_line);
                        },

                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            let current_line = buffer.char_to_line(cursor_char_idx);
                            let line_start = buffer.line_to_char(current_line);
                            let line_end = if current_line + 1 < total_lines {
                                buffer.line_to_char(current_line + 1)
                            } else {
                                buffer.len_chars()
                            };
                            clipboard = buffer.slice(line_start..line_end).to_string();
                            status_message = Some("Line copied to clipboard".into());
                            status_message_time = Some(Instant::now());
                        },

                        KeyCode::Char('v') if modifiers.contains(KeyModifiers::CONTROL) => {
                            if clipboard.is_empty() {
                                continue;
                            }
                            let char_idx = cursor_char_idx;
                            cursor_char_idx = safe_insert(&mut buffer, char_idx, &clipboard);
                            add_edit_op(&mut undo_stack, EditOp::Insert { char_idx, content: clipboard.clone() });
                            redo_stack.clear();
                            dirty_lines.insert(buffer.char_to_line(char_idx));
                        },

                        KeyCode::Char(c) => {
                            buffer.insert_char(cursor_char_idx, c);
                            add_edit_op(&mut undo_stack, EditOp::Insert { char_idx: cursor_char_idx, content: c.to_string() });
                            redo_stack.clear();
                            dirty_lines.insert(buffer.char_to_line(cursor_char_idx));
                            cursor_char_idx += 1;
                        },

                        KeyCode::Left => if cursor_char_idx > 0 { cursor_char_idx -= 1 },

                        KeyCode::Right => if cursor_char_idx < buffer.len_chars() { cursor_char_idx += 1 },

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
                                let del_start = cursor_char_idx - 1;
                                let del_end = cursor_char_idx;
                                let content = buffer.slice(del_start..del_end).to_string();
                                safe_remove(&mut buffer, del_start, content.len());
                                cursor_char_idx = del_start;
                                add_edit_op(&mut undo_stack, EditOp::Delete { char_idx: del_start, content });
                                redo_stack.clear();
                                dirty_lines.insert(buffer.char_to_line(cursor_char_idx));
                            }
                        },

                        KeyCode::Enter => {
                            buffer.insert_char(cursor_char_idx, '\n');
                            add_edit_op(&mut undo_stack, EditOp::Insert { char_idx: cursor_char_idx, content: "\n".to_string() });
                            let curr_line = buffer.char_to_line(cursor_char_idx);
                            dirty_lines.insert(curr_line);
                            dirty_lines.insert(curr_line + 1);
                            redo_stack.clear();
                            cursor_char_idx += 1;
                        },

                        _ => {}
                    },
                    InputMode::EnteringFileNameOpen => match code {
                        KeyCode::Esc => {
                            input_mode = InputMode::Editing;
                            status_message = Some("Open cancelled".to_string());
                            status_message_time = Some(Instant::now());
                        }
                        KeyCode::Enter => {
                            match open_file(&filename_input) {
                                Ok(new_buffer) => {
                                    buffer = new_buffer;
                                    cursor_char_idx = 0;
                                    current_file = Some(filename_input.clone());
                                    status_message = Some(format!("File loaded from {}", filename_input));
                                    viewport_row = 0;
                                    dirty_lines.clear();
                                    for i in 0..max_lines {
                                        dirty_lines.insert(viewport_row + i);
                                    }
                                }
                                Err(e) => {
                                    status_message = Some(format!("Error opening file: {}", e));
                                }
                            }
                            status_message_time = Some(Instant::now());
                            input_mode = InputMode::Editing;
                        }
                        KeyCode::Backspace => {
                            filename_input.pop();
                        }
                        KeyCode::Char(c) => {
                            filename_input.push(c);
                        }
                        _ => {}
                    },
                    InputMode::EnteringFileNameSave => match code {
                        KeyCode::Esc => {
                            input_mode = InputMode::Editing;
                            status_message = Some("Save cancelled".to_string());
                            status_message_time = Some(Instant::now());
                        }
                        KeyCode::Enter => {
                            match save_file(&filename_input, &buffer) {
                                Ok(_) => {
                                    current_file = Some(filename_input.clone());
                                    status_message = Some(format!("File saved to {}", filename_input));
                                }
                                Err(e) => {
                                    status_message = Some(format!("Error saving file: {}", e));
                                }
                            }
                            status_message_time = Some(Instant::now());
                            input_mode = InputMode::Editing;
                        }
                        KeyCode::Backspace => {
                            filename_input.pop();
                        }
                        KeyCode::Char(c) => {
                            filename_input.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;
    Ok(())
}
