/* 忆栈 MemStack 前端。通过 Tauri invoke 调 Rust 命令。 */

const { invoke } = window.__TAURI__.core;

const state = {
  decks: [],
  selectedDeckId: null, // null = 未选；'all' 不用，复习可用 null
  notes: [],
  editorKind: "qa",
  editingNoteId: null,
  review: {
    active: false,
    flipped: false,
    current: null,
  },
};

const $ = (id) => document.getElementById(id);

async function refreshDecks() {
  state.decks = await invoke("list_decks");
  renderDecks();
}

function renderDecks() {
  const nav = $("deck-list");
  nav.innerHTML = "";
  for (const d of state.decks) {
    const btn = document.createElement("button");
    btn.className = "deck-item" + (state.selectedDeckId === d.id ? " active" : "");
    btn.innerHTML = `<span>${escapeHtml(d.name)}</span>
      <span class="counts"><b>${d.new}</b> · <i>${d.review}</i> · ${d.learning}</span>`;
    btn.title = `新卡 ${d.new} · 待复习 ${d.review} · 学习中 ${d.learning}`;
    btn.onclick = () => selectDeck(d.id);
    nav.appendChild(btn);
  }
  if (state.decks.length === 0) {
    nav.innerHTML = `<p class="muted" style="padding:8px">还没有牌组。新建笔记会自动创建。</p>`;
  }
}

async function selectDeck(id) {
  state.selectedDeckId = id;
  renderDecks();
  const deck = state.decks.find((d) => d.id === id);
  $("main-title").textContent = deck ? deck.name : "笔记";
  $("main-sub").textContent = deck
    ? `新卡 ${deck.new} · 待复习 ${deck.review} · 学习中 ${deck.learning}`
    : "";
  $("btn-review-deck").disabled = !deck;
  state.notes = await invoke("list_notes", { deckId: id, limit: 500 });
  renderNotes();
}

function renderNotes() {
  const box = $("note-list");
  box.innerHTML = "";
  if (state.notes.length === 0) {
    box.innerHTML = `<p class="muted">这个牌组还没有笔记。点右上角「＋ 新建」。</p>`;
    return;
  }
  for (const n of state.notes) {
    const el = document.createElement("article");
    el.className = "note-card";
    el.innerHTML = `
      <div class="top">
        <div class="preview">${escapeHtml(n.preview)}</div>
      </div>
      <div class="meta">
        <span class="badge kind-${n.kind}">${kindLabel(n.kind)}</span>
        <span class="badge">${n.cardCount} 张卡</span>
        ${n.tags.map((t) => `<span class="badge">#${escapeHtml(t)}</span>`).join("")}
        <span>${escapeHtml(formatTime(n.updatedAt))}</span>
      </div>`;
    el.onclick = () => openEditor(n.id);
    box.appendChild(el);
  }
}

function kindLabel(k) {
  return { qa: "问答", cloze: "填空", code: "代码填空" }[k] || k;
}

function formatTime(iso) {
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch {
    return iso;
  }
}

function escapeHtml(s) {
  return String(s)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** 极简 Markdown：代码块 / 行内代码 / 换行。复习与预览共用。 */
function renderMarkdown(src) {
  const esc = escapeHtml(src || "");
  const parts = esc.split(/```/);
  let html = "";
  for (let i = 0; i < parts.length; i++) {
    if (i % 2 === 1) {
      // 代码块：首行可能是语言
      let body = parts[i];
      let lang = "";
      const nl = body.indexOf("\n");
      if (nl >= 0 && body.slice(0, nl).trim().length < 20 && !body.slice(0, nl).includes(" ")) {
        lang = body.slice(0, nl).trim();
        body = body.slice(nl + 1);
      }
      html += `<pre class="mono" data-lang="${lang}" style="background:#0b1015;border:1px solid var(--line);border-radius:8px;padding:12px;overflow:auto"><code>${body}</code></pre>`;
    } else {
      let t = parts[i];
      t = t.replace(/`([^`]+)`/g, "<code>$1</code>");
      html += t;
    }
  }
  return html;
}

// ---------- 编辑器 ----------

async function openEditor(noteId) {
  state.editingNoteId = noteId || null;
  $("editor-error").textContent = "";
  const deckName =
    state.decks.find((d) => d.id === state.selectedDeckId)?.name ||
    state.decks[0]?.name ||
    "";

  if (noteId) {
    const n = await invoke("get_note", { noteId });
    $("editor-title").textContent = "编辑笔记";
    setKind(n.kind);
    $("f-deck").value = n.deckName;
    $("f-tags").value = n.tags.join(", ");
    $("f-front").value = n.front;
    $("f-back").value = n.back;
    $("f-text").value = n.text;
    $("f-code").value = n.code;
    $("f-language").value = n.language;
    $("f-extra-cloze").value = n.extra;
    $("f-extra-code").value = n.extra;
  } else {
    $("editor-title").textContent = "新建笔记";
    setKind("qa");
    $("f-deck").value = deckName;
    $("f-tags").value = "";
    $("f-front").value = "";
    $("f-back").value = "";
    $("f-text").value = "";
    $("f-code").value = "";
    $("f-language").value = "";
    $("f-extra-cloze").value = "";
    $("f-extra-code").value = "";
  }
  $("editor-overlay").classList.remove("hidden");
  $("f-front").focus();
}

function closeEditor() {
  $("editor-overlay").classList.add("hidden");
}

function setKind(kind) {
  state.editorKind = kind;
  for (const tab of document.querySelectorAll(".kind-tabs .tab")) {
    tab.classList.toggle("active", tab.dataset.kind === kind);
  }
  $("pane-qa").classList.toggle("hidden", kind !== "qa");
  $("pane-cloze").classList.toggle("hidden", kind !== "cloze");
  $("pane-code").classList.toggle("hidden", kind !== "code");
}

function collectInput() {
  const tags = $("f-tags")
    .value.split(/[,，、\s]+/)
    .map((t) => t.trim())
    .filter(Boolean);
  const extra =
    state.editorKind === "cloze"
      ? $("f-extra-cloze").value
      : state.editorKind === "code"
        ? $("f-extra-code").value
        : "";
  return {
    deck: $("f-deck").value.trim(),
    kind: state.editorKind,
    front: $("f-front").value,
    back: $("f-back").value,
    text: $("f-text").value,
    code: $("f-code").value,
    language: $("f-language").value,
    extra,
    tags,
  };
}

async function saveNote() {
  $("editor-error").textContent = "";
  try {
    const input = collectInput();
    if (!input.deck) throw new Error("请填写牌组");
    if (state.editingNoteId) {
      await invoke("update_note", { noteId: state.editingNoteId, input });
    } else {
      await invoke("create_note", { input });
    }
    closeEditor();
    await refreshDecks();
    if (state.selectedDeckId != null) await selectDeck(state.selectedDeckId);
  } catch (e) {
    $("editor-error").textContent = String(e);
  }
}

// ---------- 复习 ----------

async function startReview(deckId) {
  const card = await invoke("start_review", {
    deckId: deckId ?? null,
    limit: 50,
  });
  state.review.active = true;
  state.review.flipped = false;
  $("review-overlay").classList.remove("hidden");
  $("review-done").classList.add("hidden");
  $("review-card").classList.remove("hidden");
  $("review-hint").classList.remove("hidden");
  if (!card) {
    showReviewDone({
      reviewed: 0,
      newUsed: 0,
      newCap: 0,
      reviewUsed: 0,
      reviewCap: 0,
    });
    return;
  }
  showReviewCard(card);
}

function showReviewCard(card) {
  state.review.current = card;
  state.review.flipped = false;
  $("review-front").innerHTML = renderMarkdown(card.card.front);
  $("review-back").innerHTML = renderMarkdown(card.card.back);
  $("review-back").classList.add("hidden");
  $("review-divider").classList.add("hidden");
  $("review-buttons").classList.add("hidden");
  $("review-hint").classList.remove("hidden");
  $("review-hint").textContent = "空格显示答案 · u 撤销 · q 退出";
  $("review-meta").textContent = `${stateLabel(card.card.state)} · ${kindLabel(card.card.cardType)} · 剩余 ${card.remaining}`;
  $("review-usage").textContent = `新卡 ${card.newUsed}/${card.newCap} · 复习 ${card.reviewUsed}/${card.reviewCap}`;
  renderRateButtons(card.buttons);
}

function renderRateButtons(buttons) {
  const row = $("review-buttons");
  row.innerHTML = "";
  for (const b of buttons) {
    const btn = document.createElement("button");
    btn.dataset.id = b.id;
    btn.innerHTML = `<span>${b.key} · ${b.label}</span><small>${escapeHtml(b.dueIn)}</small>`;
    btn.onclick = () => rate(b.id);
    row.appendChild(btn);
  }
}

function stateLabel(s) {
  return { new: "新卡", learning: "学习", review: "复习", relearning: "再学习" }[s] || s;
}

function flip() {
  if (!state.review.active || state.review.flipped) return;
  state.review.flipped = true;
  invoke("flip_card").catch(() => {});
  $("review-back").classList.remove("hidden");
  $("review-divider").classList.remove("hidden");
  $("review-buttons").classList.remove("hidden");
  $("review-hint").textContent = "1-4 评分 · u 撤销 · q 退出";
}

async function rate(id) {
  if (!state.review.flipped) return;
  try {
    const next = await invoke("rate_card", { rating: id, durationMs: 0 });
    if (!next) {
      const done = await invoke("review_done");
      showReviewDone(done);
      return;
    }
    showReviewCard(next);
  } catch (e) {
    $("review-hint").textContent = String(e);
  }
}

async function undo() {
  try {
    const card = await invoke("undo_review");
    if (card) showReviewCard(card);
  } catch (e) {
    $("review-hint").textContent = String(e);
  }
}

function showReviewDone(done) {
  $("review-card").classList.add("hidden");
  $("review-hint").classList.add("hidden");
  $("review-buttons").classList.add("hidden");
  $("review-done").classList.remove("hidden");
  $("review-done-text").textContent =
    `复习 ${done.reviewed} 张 · 新卡 ${done.newUsed}/${done.newCap} · 复习 ${done.reviewUsed}/${done.reviewCap}`;
  state.review.current = null;
}

async function quitReview() {
  if (state.review.active) {
    await invoke("review_done").catch(() => {});
  }
  state.review.active = false;
  $("review-overlay").classList.add("hidden");
  await refreshDecks();
  if (state.selectedDeckId != null) await selectDeck(state.selectedDeckId);
}

// ---------- 事件 ----------

function bind() {
  $("btn-new-note").onclick = () => openEditor(null);
  $("editor-close").onclick = closeEditor;
  $("editor-save").onclick = saveNote;
  for (const tab of document.querySelectorAll(".kind-tabs .tab")) {
    tab.onclick = () => setKind(tab.dataset.kind);
  }
  $("btn-review-all").onclick = () => startReview(null);
  $("btn-review-deck").onclick = () => startReview(state.selectedDeckId);
  $("review-quit").onclick = quitReview;
  $("review-done-close").onclick = quitReview;

  window.addEventListener("keydown", (e) => {
    if (!state.review.active) return;
    if (e.key === " " || e.key === "Spacebar") {
      e.preventDefault();
      flip();
    } else if (e.key === "u" || e.key === "U") {
      e.preventDefault();
      undo();
    } else if (e.key === "q" || e.key === "Q" || e.key === "Escape") {
      e.preventDefault();
      quitReview();
    } else if (state.review.flipped && ["1", "2", "3", "4"].includes(e.key)) {
      e.preventDefault();
      const map = { "1": "again", "2": "hard", "3": "good", "4": "easy" };
      rate(map[e.key]);
    }
  });
}

async function boot() {
  bind();
  await refreshDecks();
  if (state.decks.length > 0) {
    await selectDeck(state.decks[0].id);
  } else {
    $("main-title").textContent = "还没有牌组";
    $("main-sub").textContent = "点「＋ 新建」造第一张卡";
    $("btn-review-deck").disabled = true;
  }
}

boot().catch((e) => {
  console.error(e);
  $("main-title").textContent = "启动失败";
  $("main-sub").textContent = String(e);
});
