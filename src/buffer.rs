// src/buffer.rs

use ropey::Rope;
use std::fs::{write, read_to_string};
use std::io;

#[derive(Clone, Debug)]
pub enum EditOp {
    Insert { char_idx: usize, content: String },
    Delete { char_idx: usize, content: String },
}

#[derive(Clone, Debug)]
pub struct EditAction {
    pub ops: Vec<EditOp>,
    pub timestamp: std::time::Instant,
}

pub struct EditorBuffer {
    pub rope: Rope,
}

impl EditorBuffer {
    pub fn new() -> Self {
        EditorBuffer { rope: Rope::new() }
    }

    pub fn insert_char(&mut self, idx: usize, ch: char) {
        self.rope.insert_char(idx, ch);
    }

    pub fn remove(&mut self, start: usize, len: usize) {
        self.rope.remove(start..start + len);
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, idx: usize) -> ropey::RopeSlice {
        self.rope.line(idx)
    }

    pub fn char_to_line(&self, idx: usize) -> usize {
        self.rope.char_to_line(idx)
    }

    pub fn line_to_char(&self, line_idx: usize) -> usize {
        self.rope.line_to_char(line_idx)
    }

    pub fn slice<R>(&self, range: R) -> String 
    where R: std::ops::RangeBounds<usize>
    {
        let start = match range.start_bound() {
            std::ops::Bound::Included(&s) => s,
            std::ops::Bound::Excluded(&s) => s + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(&e) => e + 1,
            std::ops::Bound::Excluded(&e) => e,
            std::ops::Bound::Unbounded => self.len_chars(),
        };
        self.rope.slice(start..end).to_string()
    }
}

pub struct UndoRedoStacks {
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,
}

const GROUP_TIME_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(200);

impl UndoRedoStacks {
    pub fn new() -> Self {
        UndoRedoStacks {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn add_insert(&mut self, char_idx: usize, content: String) {
        let now = std::time::Instant::now();
        if let Some(last) = self.undo_stack.last_mut() {
            if now.duration_since(last.timestamp) < GROUP_TIME_THRESHOLD {
                last.ops.push(EditOp::Insert { char_idx, content });
                last.timestamp = now;
                return;
            }
        }
        self.undo_stack.push(EditAction {
            ops: vec![EditOp::Insert { char_idx, content }],
            timestamp: now,
        });
        self.redo_stack.clear();
    }

    pub fn add_delete(&mut self, char_idx: usize, content: String) {
        let now = std::time::Instant::now();
        if let Some(last) = self.undo_stack.last_mut() {
            if now.duration_since(last.timestamp) < GROUP_TIME_THRESHOLD {
                last.ops.push(EditOp::Delete { char_idx, content });
                last.timestamp = now;
                return;
            }
        }
        self.undo_stack.push(EditAction {
            ops: vec![EditOp::Delete { char_idx, content }],
            timestamp: now,
        });
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, buffer: &mut EditorBuffer, cursor: &mut usize, dirty_lines: &mut std::collections::HashSet<usize>) {
        if let Some(action) = self.undo_stack.pop() {
            for op in action.ops.iter().rev() {
                match op {
                    EditOp::Insert { char_idx, content } => {
                        buffer.remove(*char_idx, content.len());
                        *cursor = *char_idx;
                        dirty_lines.insert(buffer.char_to_line(*char_idx));
                    }
                    EditOp::Delete { char_idx, content } => {
                        buffer.insert_char(*char_idx, content.chars().next().unwrap()); // Slight simplification
                        *cursor = *char_idx + content.len();
                        dirty_lines.insert(buffer.char_to_line(*char_idx));
                    }
                }
            }
            self.redo_stack.push(action);
        }
    }

    pub fn redo(&mut self, buffer: &mut EditorBuffer, cursor: &mut usize, dirty_lines: &mut std::collections::HashSet<usize>) {
        if let Some(action) = self.redo_stack.pop() {
            for op in &action.ops {
                match op {
                    EditOp::Insert { char_idx, content } => {
                        for c in content.chars() {
                            buffer.insert_char(*char_idx, c);
                        }
                        *cursor = *char_idx + content.len();
                        dirty_lines.insert(buffer.char_to_line(*char_idx));
                    }
                    EditOp::Delete { char_idx, content } => {
                        buffer.remove(*char_idx, content.len());
                        *cursor = *char_idx;
                        dirty_lines.insert(buffer.char_to_line(*char_idx));
                    }
                }
            }
            self.undo_stack.push(action);
        }
    }
}

// File IO functions
pub fn save_file(path: &str, buffer: &EditorBuffer) -> io::Result<()> {
    write(path, buffer.slice(..))
}

pub fn open_file(path: &str) -> io::Result<EditorBuffer> {
    let content = read_to_string(path)?;
    Ok(EditorBuffer {
        rope: Rope::from_str(&content),
    })
}
