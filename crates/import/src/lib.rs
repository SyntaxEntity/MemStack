//! 卡片导入：Markdown 文件夹、Anki `.apkg`。
//!
//! Markdown 格式（见 docs/方案.md）：
//!
//! ```text
//! ---
//! deck: 算法
//! tags: [leetcode, 二分]
//! type: qa | cloze | code   // 可省，自动识别
//! language: rust            // type=code 时的高亮语言
//! ---
//!
//! 正面内容（Markdown，可含代码块）
//!
//! ---
//!
//! 背面内容
//!
//! ***
//!
//! 第二张卡……
//! ```
//!
//! - 文件级 frontmatter 写在最前，对全文件生效
//! - `***` 单独成行 = 下一张卡
//! - 问答用 `---` 分正反面；填空 / 代码填空整段就是正文，用 `{{cN::答案}}` 挖空
//! - 也兼容 `正面：` / `背面：` 标注写法

pub mod anki;
mod error;
mod parse;

use std::path::Path;

pub use anki::{parse_apkg, AnkiImport, ImportedCard, ImportedNote, ImportedReview};
pub use error::{Error, Result};
pub use parse::{parse_markdown_cards, NoteDraft, ParsedFile};

/// 从文件读入并解析。`default_deck` 覆盖 frontmatter 里的 deck。
pub fn import_markdown_file(path: &str, default_deck: Option<&str>) -> Result<Vec<NoteDraft>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_string(),
        source,
    })?;
    let file = parse_markdown_cards(&text, default_deck)?;
    Ok(file.notes)
}

/// 从目录批量导入 `*.md`（递归）。
///
/// 未写 deck 时，用文件所在子目录名作默认牌组；根目录下的文件用 `dir` 的名字。
pub fn import_markdown_dir(dir: &str, default_deck: Option<&str>) -> Result<Vec<NoteDraft>> {
    let mut paths = Vec::new();
    collect_md(Path::new(dir), &mut paths)?;
    paths.sort();

    let root_name = Path::new(dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(dir)
        .to_string();

    let mut all = Vec::new();
    for path in &paths {
        let deck = default_deck
            .map(str::to_string)
            .unwrap_or_else(|| deck_from_path(path, dir, &root_name));
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        let file = parse_markdown_cards(&text, Some(&deck))?;
        for mut note in file.notes {
            note.source = Some(path.display().to_string());
            all.push(note);
        }
    }
    Ok(all)
}

fn collect_md(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.display().to_string(),
        source,
    })?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_md(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md")
            && path.file_name().and_then(|n| n.to_str()) != Some("README.md")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// `algo/二分.md` → `algo`；根下的 `foo.md` → root_name。
fn deck_from_path(path: &Path, root: &str, root_name: &str) -> String {
    let parent = path.parent().unwrap_or(Path::new(root));
    match parent.file_name().and_then(|n| n.to_str()) {
        Some(name) if parent != Path::new(root) => name.to_string(),
        _ => root_name.to_string(),
    }
}
