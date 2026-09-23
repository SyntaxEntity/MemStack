//! 造卡起手模板：给常见记忆卡型填好骨架，减少空白页。

use crate::model::NoteContent;

/// 题型分类（与 [`NoteContent`] 对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Qa,
    Cloze,
    Code,
}

impl TemplateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TemplateKind::Qa => "qa",
            TemplateKind::Cloze => "cloze",
            TemplateKind::Code => "code",
        }
    }
}

/// 一张笔记模板。
#[derive(Debug, Clone, Copy)]
pub struct NoteTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub kind: TemplateKind,
    /// 默认标签
    pub tags: &'static [&'static str],
    pub front: &'static str,
    pub back: &'static str,
    pub text: &'static str,
    pub code: &'static str,
    pub language: &'static str,
    pub extra: &'static str,
}

impl NoteTemplate {
    /// 转成可保存的内容（可再改字段）。
    pub fn content(&self) -> NoteContent {
        match self.kind {
            TemplateKind::Qa => NoteContent::QuestionAnswer {
                front: self.front.to_string(),
                back: self.back.to_string(),
            },
            TemplateKind::Cloze => NoteContent::Cloze {
                text: self.text.to_string(),
                extra: self.extra.to_string(),
            },
            TemplateKind::Code => NoteContent::CodeCloze {
                code: self.code.to_string(),
                language: self.language.to_string(),
                extra: self.extra.to_string(),
            },
        }
    }
}

/// 内置模板。
pub const TEMPLATES: &[NoteTemplate] = &[
    NoteTemplate {
        id: "concept",
        name: "概念问答",
        summary: "术语 → 定义；正面问，背面答",
        kind: TemplateKind::Qa,
        tags: &["概念"],
        front: "什么是 ____？",
        back: "____ 是 ……。核心性质：……。",
        text: "",
        code: "",
        language: "",
        extra: "",
    },
    NoteTemplate {
        id: "command",
        name: "命令速查",
        summary: "工具命令；正面场景，背面命令 + 要点",
        kind: TemplateKind::Qa,
        tags: &["命令"],
        front: "如何用 ____ 做 ____？",
        back: "```bash\n# 命令\n```\n要点：……",
        text: "",
        code: "",
        language: "",
        extra: "",
    },
    NoteTemplate {
        id: "algo-pattern",
        name: "算法模板",
        summary: "代码填空挖关键一步；适合二分、双指针等",
        kind: TemplateKind::Code,
        tags: &["算法"],
        front: "",
        back: "",
        text: "",
        code: "```rust\nfn solve() {\n    // 思路：……\n    let m = l + ({{c1::r - l}}) / 2;\n}\n```",
        language: "rust",
        extra: "模板名：……\n时间 O(?) · 空间 O(?)",
    },
    NoteTemplate {
        id: "sql-pattern",
        name: "SQL 模式",
        summary: "SQL 填空；挖连接条件、聚合或窗口",
        kind: TemplateKind::Code,
        tags: &["sql"],
        front: "",
        back: "",
        text: "",
        code: "```sql\nSELECT {{c1::COUNT(*)}}\nFROM orders\nWHERE created_at >= {{c2::CURRENT_DATE - INTERVAL '7 day'}}\n```",
        language: "sql",
        extra: "场景：……",
    },
    NoteTemplate {
        id: "cloze-def",
        name: "概念填空",
        summary: "一句话定义挖 1–2 个空",
        kind: TemplateKind::Cloze,
        tags: &["概念"],
        front: "",
        back: "",
        text: "____ 是一种 {{c1::……}}，主要用于 {{c2::……}}。",
        code: "",
        language: "",
        extra: "",
    },
    NoteTemplate {
        id: "bug-postmortem",
        name: "踩坑复盘",
        summary: "现象 → 原因 → 修法；正面现象，背面原因",
        kind: TemplateKind::Qa,
        tags: &["踩坑"],
        front: "现象：……\n环境：……",
        back: "原因：……\n修法：……\n如何避免：……",
        text: "",
        code: "",
        language: "",
        extra: "",
    },
];

/// 按 id 取模板。
pub fn template(id: &str) -> Option<&'static NoteTemplate> {
    TEMPLATES.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_templates_resolve() {
        assert!(TEMPLATES.len() >= 6);
        let t = template("algo-pattern").unwrap();
        assert_eq!(t.kind, TemplateKind::Code);
        assert!(t.code.contains("{{c1::"));
        let c = t.content();
        assert!(matches!(c, NoteContent::CodeCloze { .. }));
    }

    #[test]
    fn unknown_template_is_none() {
        assert!(template("nope").is_none());
    }
}
