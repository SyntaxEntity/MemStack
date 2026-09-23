//! 桌面壳命令层的错误，映射给前端字符串。

use std::fmt;

#[derive(Debug)]
pub struct CmdError(pub String);

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CmdError {}

impl From<memstack_storage::Error> for CmdError {
    fn from(e: memstack_storage::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<memstack_core::Error> for CmdError {
    fn from(e: memstack_core::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<String> for CmdError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CmdError {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<CmdError> for tauri::ipc::InvokeError {
    fn from(e: CmdError) -> Self {
        tauri::ipc::InvokeError::from(e.0)
    }
}

pub type Result<T> = std::result::Result<T, CmdError>;
