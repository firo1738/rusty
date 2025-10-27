// src/input.rs

use crossterm::event::{read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crate::buffer::EditorBuffer;
use std::collections::HashSet;
use std::io::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Editing,
    Finding,
    EnteringFileNameOpen,
    EnteringFileNameSave,
}

#[derive(Debug)]
pub enum Command {
    Quit,
    InsertChar(char),
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    Backspace,
    InsertNewline,
    Undo,
    Redo,
    StartFind,
    ConfirmFind,
    StartOpenFile,
    ConfirmOpenFile,
    StartSaveFile,
    ConfirmSaveFile,
}

pub struct InputHandler {
    pub mode: InputMode,
    pub filename_input: String,
    pub find_input: String,
    pub confirmed_find_term: Option<String>,
}

impl InputHandler {
    pub fn new() -> Self {
        InputHandler {
            mode: InputMode::Editing,
            filename_input: String::new(),
            find_input: String::new(),
            confirmed_find_term: None,
        }
    }

    pub fn get_mode(&self) -> &InputMode {
        &self.mode
    }

    /// Reads and parses command input from terminal.
    /// Returns Some(Command) if an actionable command is parsed.
    pub fn process_input(&mut self) -> Result<Option<Command>> {
        while let Event::Key(key_event) = read()? {
            if key_event.kind != KeyEventKind::Press {
                continue;
            }
            match self.mode {
                InputMode::Editing => {
                    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                        match key_event.code {
                            KeyCode::Char('q') => return Ok(Some(Command::Quit)),
                            KeyCode::Char('z') => return Ok(Some(Command::Undo)),
                            KeyCode::Char('y') => return Ok(Some(Command::Redo)),
                            KeyCode::Char('f') => return Ok(Some(Command::StartFind)),
                            KeyCode::Char('o') => return Ok(Some(Command::StartOpenFile)),
                            KeyCode::Char('s') => return Ok(Some(Command::StartSaveFile)),
                            KeyCode::Left => return Ok(Some(Command::MoveLeft)),
                            KeyCode::Right => return Ok(Some(Command::MoveRight)),
                            KeyCode::Up => return Ok(Some(Command::MoveUp)),
                            KeyCode::Down => return Ok(Some(Command::MoveDown)),
                            _ => {}
                        }
                    }
                    match key_event.code {
                        KeyCode::Backspace => return Ok(Some(Command::Backspace)),
                        KeyCode::Enter => return Ok(Some(Command::InsertNewline)),
                        KeyCode::Char(c) => return Ok(Some(Command::InsertChar(c))),
                        _ => {}
                    }
                }
                InputMode::Finding => {
                    match key_event.code {
                        KeyCode::Esc => {
                            self.confirmed_find_term = None;
                            self.mode = InputMode::Editing;
                            return Ok(None);
                        }
                        KeyCode::Enter => {
                            if !self.find_input.is_empty() {
                                self.confirmed_find_term = Some(self.find_input.clone());
                            } else {
                                self.confirmed_find_term = None;
                            }
                            self.mode = InputMode::Editing;
                            return Ok(Some(Command::ConfirmFind));
                        }
                        KeyCode::Backspace => {
                            self.find_input.pop();
                            return Ok(None);
                        }
                        KeyCode::Char(c) => {
                            self.find_input.push(c);
                            return Ok(None);
                        }
                        _ => {}
                    }
                }
                InputMode::EnteringFileNameOpen => match key_event.code {
                    KeyCode::Esc => {
                        self.mode = InputMode::Editing;
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        self.mode = InputMode::Editing;
                        return Ok(Some(Command::ConfirmOpenFile));
                    }
                    KeyCode::Backspace => {
                        self.filename_input.pop();
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        self.filename_input.push(c);
                        return Ok(None);
                    }
                    _ => {}
                },
                InputMode::EnteringFileNameSave => match key_event.code {
                    KeyCode::Esc => {
                        self.mode = InputMode::Editing;
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        self.mode = InputMode::Editing;
                        return Ok(Some(Command::ConfirmSaveFile));
                    }
                    KeyCode::Backspace => {
                        self.filename_input.pop();
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        self.filename_input.push(c);
                        return Ok(None);
                    }
                    _ => {}
                },
            }
        }
        Ok(None)
    }

    pub fn start_find(&mut self) {
        self.mode = InputMode::Finding;
        self.find_input.clear();
        self.confirmed_find_term = None;
    }

    pub fn start_open_file(&mut self) {
        self.mode = InputMode::EnteringFileNameOpen;
        self.filename_input.clear();
    }

    pub fn start_save_file(&mut self) {
        self.mode = InputMode::EnteringFileNameSave;
        self.filename_input.clear();
    }

    pub fn confirm_find(&mut self, buffer: &EditorBuffer, dirty_lines: &mut std::collections::HashSet<usize>) {
        if self.find_input.is_empty() {
            self.confirmed_find_term = None;
        } else {
            self.confirmed_find_term = Some(self.find_input.clone());
            let total_lines = buffer.len_lines();
            dirty_lines.extend(0..total_lines); // Only add valid line indexes for redraw
        }
        self.mode = InputMode::Editing;
    }


    pub fn confirm_open_file(&mut self) -> Option<String> {
        if self.filename_input.is_empty() {
            None
        } else {
            Some(self.filename_input.clone())
        }
    }

    pub fn confirm_save_file(&mut self) -> Option<String> {
        if self.filename_input.is_empty() {
            None
        } else {
            Some(self.filename_input.clone())
        }
    }
}
