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
  await ensureTemplates();
  $("f-template").value = "";
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

// ---------- 统计 ----------

async function openStats() {
  try {
    const s = await invoke("stats_panel");
    renderStats(s);
    $("stats-overlay").classList.remove("hidden");
  } catch (e) {
    console.error(e);
    alert("统计加载失败：" + e);
  }
}

function renderStats(s) {
  const acc =
    s.accuracy30d == null ? "—" : Math.round(s.accuracy30d * 100) + "%";
  $("stats-summary").innerHTML = `
    <div class="cell"><b>${s.decks}</b><span>牌组</span></div>
    <div class="cell"><b>${s.notes}</b><span>笔记</span></div>
    <div class="cell"><b>${s.cards}</b><span>卡片</span></div>
    <div class="cell"><b>${s.dueNow}</b><span>待复习</span></div>
    <div class="cell"><b>${s.new} · ${s.learning}</b><span>新卡 · 学习中</span></div>
    <div class="cell"><b>${s.reviews30d}</b><span>近 30 天复习</span></div>
    <div class="cell"><b>${acc}</b><span>近 30 天正确率</span></div>
    <div class="cell"><b>${s.streakDays}</b><span>连续天数</span></div>
  `;

  const heat = $("stats-heatmap");
  heat.innerHTML = "";
  for (const day of s.heatmap) {
    const lvl = heatLevel(day.count);
    const el = document.createElement("i");
    el.className = "hm-" + lvl;
    el.title = `${day.date} · ${day.count} 次`;
    heat.appendChild(el);
  }

  const fc = $("stats-forecast");
  fc.innerHTML = "";
  const max = Math.max(1, ...s.forecast.map((d) => d.due));
  for (const day of s.forecast) {
    const col = document.createElement("div");
    col.className = "col";
    const h = Math.max(2, Math.round((day.due / max) * 100));
    col.innerHTML = `<span class="val">${day.due}</span>
      <div class="bar" style="height:${h}%"></div>
      <span class="lab">${day.date.slice(5)}</span>`;
    col.title = `${day.date} · ${day.due} 张到期`;
    fc.appendChild(col);
  }
}

function heatLevel(n) {
  if (n <= 0) return 0;
  if (n <= 4) return 1;
  if (n <= 14) return 2;
  if (n <= 34) return 3;
  return 4;
}

// ---------- 模板 ----------

async function ensureTemplates() {
  if (state.templates) return;
  state.templates = await invoke("list_templates");
  const sel = $("f-template");
  sel.innerHTML = `<option value="">— 空白 —</option>`;
  for (const t of state.templates) {
    const opt = document.createElement("option");
    opt.value = t.id;
    opt.textContent = `${t.name}（${kindLabel(t.kind)}）`;
    opt.title = t.summary;
    sel.appendChild(opt);
  }
}

function applyTemplate(id) {
  const t = (state.templates || []).find((x) => x.id === id);
  if (!t) return;
  setKind(t.kind);
  $("f-front").value = t.front;
  $("f-back").value = t.back;
  $("f-text").value = t.text;
  $("f-code").value = t.code;
  $("f-language").value = t.language;
  $("f-extra-cloze").value = t.extra;
  $("f-extra-code").value = t.extra;
  if (!$("f-tags").value.trim()) {
    $("f-tags").value = t.tags.join(", ");
  }
}

// ---------- 事件 ----------

function bind() {
  $("btn-new-note").onclick = () => openEditor(null);
  $("editor-close").onclick = closeEditor;
  $("editor-save").onclick = saveNote;
  $("f-template").onchange = () => applyTemplate($("f-template").value);
  for (const tab of document.querySelectorAll(".kind-tabs .tab")) {
    tab.onclick = () => setKind(tab.dataset.kind);
  }
  $("btn-review-all").onclick = () => startReview(null);
  $("btn-review-deck").onclick = () => startReview(state.selectedDeckId);
  $("btn-stats").onclick = openStats;
  $("stats-close").onclick = () => $("stats-overlay").classList.add("hidden");
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
