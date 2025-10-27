mod buffer;
mod input;
mod render;

use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use std::io::{stdout, Result};
use crate::buffer::{EditorBuffer, UndoRedoStacks};
use crate::input::{InputHandler, InputMode, Command};
use crate::render::{Renderer, VirtualScreen};
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let (cols, rows) = crossterm::terminal::size()?;
    let max_lines = (rows - 2) as usize;

    // State setup
    let mut buffer = EditorBuffer::new();
    let mut undo_redo = UndoRedoStacks::new();
    let mut input_handler = InputHandler::new();
    let mut renderer = Renderer::new(max_lines);

    let mut viewport_row = 0;
    let mut cursor_char_idx = 0;
    let mut dirty_lines = (0..max_lines).collect::<HashSet<_>>();

    let mut cursor_visible = true;
    let mut last_cursor_toggle = Instant::now();
    const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

    'mainloop: loop {
        // Blink cursor timing
        if last_cursor_toggle.elapsed() >= CURSOR_BLINK_INTERVAL {
            cursor_visible = !cursor_visible;
            last_cursor_toggle = Instant::now();
        }

        // Calculate current line and cursor col
        let current_line = buffer.char_to_line(cursor_char_idx);
        let line_start_char_idx = buffer.line_to_char(current_line);
        let cursor_col = cursor_char_idx.saturating_sub(line_start_char_idx);

        // Adjust viewport for cursor
        if current_line < viewport_row {
            viewport_row = current_line;
            dirty_lines.extend(viewport_row..viewport_row+max_lines);
        } else if current_line >= viewport_row + max_lines {
            viewport_row = current_line - max_lines + 1;
            dirty_lines.extend(viewport_row..viewport_row+max_lines);
        }

        // Rendering
        renderer.render(
            &mut stdout,
            &buffer,
            &dirty_lines,
            viewport_row,
            max_lines,
            cursor_col,
            current_line,
            cursor_visible,
            input_handler.get_mode(),
            &input_handler.filename_input,
            &input_handler.find_input,
            &input_handler.confirmed_find_term,
        )?;
        dirty_lines.clear();

        // Input handling
        if let Some(command) = input_handler.process_input()? {
            match command {
                Command::Quit => break 'mainloop,
                Command::InsertChar(c) => {
                    buffer.insert_char(cursor_char_idx, c);
                    undo_redo.add_insert(cursor_char_idx, c.to_string());
                    cursor_char_idx += 1;
                    dirty_lines.insert(buffer.char_to_line(cursor_char_idx));
                }
                Command::MoveLeft => { if cursor_char_idx > 0 { cursor_char_idx -= 1; } }
                Command::MoveRight => { if cursor_char_idx < buffer.len_chars() { cursor_char_idx += 1; } }
                Command::MoveUp => {
                    if current_line > 0 {
                        let target_line = current_line - 1;
                        let target_line_start = buffer.line_to_char(target_line);
                        let target_line_len = buffer.line(target_line).len_chars();
                        let new_col = cursor_col.min(target_line_len.saturating_sub(1));
                        cursor_char_idx = target_line_start + new_col;
                    }
                }
                Command::MoveDown => {
                    if current_line + 1 < buffer.len_lines() {
                        let target_line = current_line + 1;
                        let target_line_start = buffer.line_to_char(target_line);
                        let target_line_len = buffer.line(target_line).len_chars();
                        let new_col = cursor_col.min(target_line_len.saturating_sub(1));
                        cursor_char_idx = target_line_start + new_col;
                    }
                }
                Command::Backspace => {
                    if cursor_char_idx > 0 {
                        let del_start = cursor_char_idx - 1;
                        let content = buffer.slice(del_start..cursor_char_idx);
                        buffer.remove(del_start, content.len());
                        cursor_char_idx = del_start;
                        undo_redo.add_delete(del_start, content);
                        dirty_lines.insert(buffer.char_to_line(cursor_char_idx));
                    }
                }
                Command::InsertNewline => {
                    buffer.insert_char(cursor_char_idx, '\n');
                    undo_redo.add_insert(cursor_char_idx, "\n".to_string());
                    let curr_line = buffer.char_to_line(cursor_char_idx);
                    dirty_lines.insert(curr_line);
                    dirty_lines.insert(curr_line + 1);
                    cursor_char_idx += 1;
                }
                Command::Undo => undo_redo.undo(&mut buffer, &mut cursor_char_idx, &mut dirty_lines),
                Command::Redo => undo_redo.redo(&mut buffer, &mut cursor_char_idx, &mut dirty_lines),
                Command::StartFind => input_handler.start_find(),
                Command::ConfirmFind => input_handler.confirm_find(&buffer, &mut dirty_lines),
                Command::StartOpenFile => input_handler.start_open_file(),
                Command::ConfirmOpenFile => {
                    if let Some(path) = input_handler.confirm_open_file() {
                        if let Ok(new_buffer) = buffer::open_file(&path) {
                            buffer = new_buffer;
                            cursor_char_idx = 0;
                            viewport_row = 0;
                            dirty_lines.extend(0..max_lines);
                        }
                    }
                },
                Command::StartSaveFile => input_handler.start_save_file(),
                Command::ConfirmSaveFile => {
                    if let Some(path) = input_handler.confirm_save_file() {
                        let _ = buffer::save_file(&path, &buffer);
                    }
                },
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;
    Ok(())
}
