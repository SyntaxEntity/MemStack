-- 忆栈 MemStack 核心表。复习记录只增不改。

CREATE TABLE IF NOT EXISTS decks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    parent_id  INTEGER REFERENCES decks (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id    INTEGER NOT NULL REFERENCES decks (id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    tags       TEXT    NOT NULL DEFAULT '[]',
    source     TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_deck ON notes (deck_id);

CREATE TABLE IF NOT EXISTS cards (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id     INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    deck_id     INTEGER NOT NULL REFERENCES decks (id) ON DELETE CASCADE,
    card_type   TEXT    NOT NULL,
    ordinal     INTEGER NOT NULL,
    front       TEXT    NOT NULL,
    back        TEXT    NOT NULL,
    state       TEXT    NOT NULL,
    step        INTEGER NOT NULL DEFAULT 0,
    stability   REAL,
    difficulty  REAL,
    due         INTEGER NOT NULL,
    last_review INTEGER,
    reps        INTEGER NOT NULL DEFAULT 0,
    lapses      INTEGER NOT NULL DEFAULT 0,
    suspended   INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (note_id, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_cards_due ON cards (suspended, due);
CREATE INDEX IF NOT EXISTS idx_cards_deck ON cards (deck_id, state);

CREATE TABLE IF NOT EXISTS review_logs (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id            INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    reviewed_at        INTEGER NOT NULL,
    rating             INTEGER NOT NULL,
    state_before       TEXT    NOT NULL,
    step_before        INTEGER NOT NULL,
    stability_before   REAL,
    difficulty_before  REAL,
    interval_days      REAL    NOT NULL,
    elapsed_days       INTEGER NOT NULL,
    duration_ms        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_review_logs_card ON review_logs (card_id, reviewed_at);
CREATE INDEX IF NOT EXISTS idx_review_logs_time ON review_logs (reviewed_at);

CREATE TABLE IF NOT EXISTS deck_params (
    deck_id            INTEGER PRIMARY KEY REFERENCES decks (id) ON DELETE CASCADE,
    fsrs_params        TEXT    NOT NULL DEFAULT '[]',
    desired_retention  REAL    NOT NULL DEFAULT 0.9,
    max_interval_days  INTEGER NOT NULL DEFAULT 36500,
    learning_steps     TEXT    NOT NULL DEFAULT '[1,10]',
    relearning_steps   TEXT    NOT NULL DEFAULT '[10]',
    new_cards_per_day  INTEGER NOT NULL DEFAULT 20,
    reviews_per_day    INTEGER NOT NULL DEFAULT 200
);
