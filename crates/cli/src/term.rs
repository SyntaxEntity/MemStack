//! 终端原始按键：空格翻面、1-4 评分。

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::stdout;

/// 复习流要的键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Flip,
    Rate(u8),
    Undo,
    Quit,
    Ignore,
}

pub struct RawGuard;

impl RawGuard {
    pub fn enter() -> std::io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

pub fn read_key() -> std::io::Result<Key> {
    loop {
        match event::read()? {
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                modifiers,
                ..
            }) => {
                if modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(code, KeyCode::Char('c') | KeyCode::Char('C'))
                {
                    return Ok(Key::Quit);
                }
                return Ok(match code {
                    KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('j') => Key::Flip,
                    KeyCode::Char(c @ '1'..='4') => Key::Rate(c as u8 - b'0'),
                    KeyCode::Char('u') | KeyCode::Char('U') => Key::Undo,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Key::Quit,
                    _ => Key::Ignore,
                });
            }
            _ => continue,
        }
    }
}

pub fn clear() {
    let mut out = stdout();
    let _ = crossterm::execute!(
        out,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    );
}

pub fn print_banner(line: &str) {
    println!("\x1b[1m{line}\x1b[0m");
    println!("{}", "─".repeat(line.chars().count().min(60)));
}

pub fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[0m")
}
