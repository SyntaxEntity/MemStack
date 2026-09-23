"use strict";
// 忆栈 MemStack：划词造卡。依赖 PATH 上的 memstack CLI。
const vscode = require("vscode");
const { spawn } = require("child_process");

const LANG_MAP = {
  rust: "rust",
  python: "python",
  go: "go",
  c: "c",
  cpp: "cpp",
  java: "java",
  javascript: "javascript",
  typescript: "typescript",
  sql: "sql",
  shellscript: "bash",
  lua: "lua",
  ruby: "ruby",
  toml: "toml",
  yaml: "yaml",
  json: "json",
  markdown: "",
};

function cfg() {
  const c = vscode.workspace.getConfiguration("memstack");
  return {
    db: (c.get("db") || "").trim(),
    defaultDeck: c.get("defaultDeck") || "收件箱",
    bin: c.get("bin") || "memstack",
  };
}

function selectionText() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return null;
  }
  const sel = editor.selection;
  if (sel.isEmpty) {
    return null;
  }
  return editor.document.getText(sel);
}

function languageId() {
  const editor = vscode.window.activeTextEditor;
  const id = editor?.document?.languageId || "";
  return LANG_MAP[id] !== undefined ? LANG_MAP[id] : id;
}

function submit(deck, tags, payload) {
  const { db, bin } = cfg();
  const args = [];
  if (db) {
    args.push("--db", db);
  }
  args.push("add-json", "--deck", deck);
  if (tags && tags.length) {
    args.push("--tags", tags.join(","));
  }
  args.push("-");

  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, { stdio: ["pipe", "pipe", "pipe"] });
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve(out.trim());
      } else {
        reject(new Error(err.trim() || out.trim() || `exit ${code}`));
      }
    });
    child.stdin.write(JSON.stringify(payload));
    child.stdin.end();
  });
}

async function askDeck() {
  const { defaultDeck } = cfg();
  const deck = await vscode.window.showInputBox({
    prompt: "牌组",
    value: defaultDeck,
  });
  return deck === undefined ? null : deck.trim() || defaultDeck;
}

async function askTags() {
  const raw = await vscode.window.showInputBox({
    prompt: "标签（逗号分隔，可空）",
    value: "",
  });
  if (raw === undefined) {
    return null;
  }
  return raw
    .split(/[,，、\s]+/)
    .map((t) => t.trim())
    .filter(Boolean);
}

async function finish(payload) {
  const deck = await askDeck();
  if (!deck) {
    return;
  }
  const tags = await askTags();
  if (!tags) {
    return;
  }
  try {
    const msg = await submit(deck, tags, payload);
    vscode.window.showInformationMessage(`[MemStack] ${msg}`);
  } catch (e) {
    vscode.window.showErrorMessage(`[MemStack] 造卡失败：${e.message}`);
  }
}

async function cardQa() {
  const front = selectionText();
  if (!front) {
    vscode.window.showWarningMessage("[MemStack] 先选中一段作为正面");
    return;
  }
  const back = await vscode.window.showInputBox({ prompt: "背面" });
  if (!back || !back.trim()) {
    return;
  }
  await finish({ kind: "qa", front, back: back.trim() });
}

async function cardCode() {
  const code = selectionText();
  if (!code) {
    vscode.window.showWarningMessage("[MemStack] 先选中代码");
    return;
  }
  const needle = await vscode.window.showInputBox({
    prompt: "选区中要挖空的原文（取消=整段挖空）",
  });
  let edited = code;
  if (needle === undefined) {
    return;
  }
  if (needle.trim()) {
    const idx = code.indexOf(needle);
    if (idx < 0) {
      vscode.window.showWarningMessage("[MemStack] 选区里没有该片段");
      return;
    }
    edited =
      code.slice(0, idx) + "{{c1::" + needle + "}}" + code.slice(idx + needle.length);
  } else {
    edited = `{{c1::${code}}}`;
  }
  const extra = (await vscode.window.showInputBox({ prompt: "补充（可空）" })) || "";
  await finish({
    kind: "code",
    code: edited,
    language: languageId(),
    extra,
  });
}

async function cardCloze() {
  const text = selectionText();
  if (!text) {
    vscode.window.showWarningMessage("[MemStack] 先选中一段");
    return;
  }
  const body = text.includes("{{c") ? text : `{{c1::${text}}}`;
  const extra = (await vscode.window.showInputBox({ prompt: "补充（可空）" })) || "";
  await finish({ kind: "cloze", text: body, extra });
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("memstack.cardQa", cardQa),
    vscode.commands.registerCommand("memstack.cardCode", cardCode),
    vscode.commands.registerCommand("memstack.cardCloze", cardCloze)
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
