CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(name) > 0),
    created_at TEXT NOT NULL,
    archived_at TEXT
);

CREATE TABLE sources (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'utterance')),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    idempotency_key TEXT,
    original_filename TEXT,
    ext TEXT NOT NULL CHECK (length(ext) > 0 AND ext = lower(ext)),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    evidence_relpath TEXT NOT NULL CHECK (length(evidence_relpath) > 0),
    imported_at TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('imported', 'parsing', 'parsed', 'failed', 'reviewed')),
    parse_error_code TEXT,
    latest_attempt_id TEXT REFERENCES parse_attempts(id) DEFERRABLE INITIALLY DEFERRED,
    CHECK (
        (kind = 'file' AND idempotency_key IS NULL AND original_filename IS NOT NULL)
        OR
        (kind = 'utterance' AND idempotency_key IS NOT NULL AND original_filename IS NULL AND ext = 'txt')
    ),
    CHECK (
        (state = 'failed' AND parse_error_code IS NOT NULL)
        OR
        (state <> 'failed' AND parse_error_code IS NULL)
    )
);

CREATE UNIQUE INDEX sources_file_hash
    ON sources(content_hash)
    WHERE kind = 'file';

CREATE UNIQUE INDEX sources_idempotency_key
    ON sources(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE parse_attempts (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(id),
    agent_session_id TEXT NOT NULL,
    backend_id TEXT NOT NULL CHECK (length(backend_id) > 0),
    backend_version TEXT,
    model_id TEXT,
    prompt_hash TEXT NOT NULL CHECK (length(prompt_hash) = 64),
    tool_surface_version TEXT NOT NULL CHECK (length(tool_surface_version) > 0),
    effective_capability_hash TEXT NOT NULL CHECK (length(effective_capability_hash) = 64),
    app_version TEXT NOT NULL CHECK (length(app_version) > 0),
    started_at TEXT NOT NULL,
    ended_at TEXT,
    outcome TEXT CHECK (outcome IN (
        'completed', 'completed_with_gaps', 'failed', 'timeout',
        'interrupted', 'cancelled', 'protocol_violation'
    )),
    error_code TEXT,
    reported_item_count INTEGER CHECK (reported_item_count >= 0),
    unparsed_note TEXT,
    reported_total_minor INTEGER CHECK (abs(reported_total_minor) <= 1000000000000000),
    reported_total_currency TEXT,
    reported_total_kind TEXT CHECK (reported_total_kind IN ('expense_total', 'income_total', 'net_change')),
    reported_total_evidence_text TEXT,
    CHECK (
        (reported_total_minor IS NULL AND reported_total_currency IS NULL
            AND reported_total_kind IS NULL AND reported_total_evidence_text IS NULL)
        OR
        (reported_total_minor IS NOT NULL AND reported_total_currency IS NOT NULL
            AND reported_total_kind IS NOT NULL AND reported_total_evidence_text IS NOT NULL
            AND length(reported_total_evidence_text) > 0)
    ),
    CHECK (
        (outcome IS NULL AND error_code IS NULL AND ended_at IS NULL)
        OR
        (outcome IN ('completed', 'completed_with_gaps') AND error_code IS NULL AND ended_at IS NOT NULL)
        OR
        (outcome IN ('failed', 'timeout', 'interrupted', 'cancelled', 'protocol_violation')
            AND error_code IS NOT NULL AND ended_at IS NOT NULL)
    )
);

CREATE INDEX parse_attempts_source ON parse_attempts(source_id, started_at);

CREATE TABLE draft_transactions (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(id),
    evidence_text TEXT NOT NULL CHECK (length(evidence_text) > 0),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal > 0),
    evidence_span_start INTEGER,
    evidence_span_end INTEGER,
    attempt_id TEXT NOT NULL REFERENCES parse_attempts(id),
    drafted_json TEXT NOT NULL CHECK (json_valid(drafted_json)),
    voided_at TEXT,
    discarded_at TEXT,
    occurred_on TEXT NOT NULL CHECK (length(occurred_on) = 10),
    amount_minor INTEGER NOT NULL CHECK (abs(amount_minor) <= 1000000000000000),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    base_amount_minor INTEGER CHECK (abs(base_amount_minor) <= 1000000000000000),
    base_currency TEXT CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    rate_ppm INTEGER CHECK (rate_ppm > 0 AND rate_ppm <= 1000000000000000),
    direction TEXT NOT NULL CHECK (direction IN ('expense', 'income', 'transfer')),
    merchant TEXT NOT NULL CHECK (length(merchant) > 0),
    category TEXT,
    channel TEXT,
    confidence INTEGER CHECK (confidence BETWEEN 0 AND 100),
    created_at TEXT NOT NULL,
    consumed_at TEXT,
    CHECK (
        (base_amount_minor IS NULL AND base_currency IS NULL AND rate_ppm IS NULL)
        OR
        (base_amount_minor IS NOT NULL AND base_currency IS NOT NULL AND rate_ppm IS NOT NULL)
    ),
    CHECK (
        (evidence_span_start IS NULL AND evidence_span_end IS NULL)
        OR
        (evidence_span_start IS NOT NULL AND evidence_span_end IS NOT NULL
            AND evidence_span_start >= 0 AND evidence_span_start < evidence_span_end)
    ),
    CHECK (
        (voided_at IS NOT NULL) + (discarded_at IS NOT NULL) + (consumed_at IS NOT NULL) <= 1
    ),
    UNIQUE(attempt_id, source_ordinal)
);

CREATE INDEX draft_transactions_source ON draft_transactions(source_id);
CREATE INDEX draft_transactions_active ON draft_transactions(attempt_id, source_ordinal)
    WHERE voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL;

CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT REFERENCES sources(id),
    source_draft_id TEXT UNIQUE REFERENCES draft_transactions(id),
    evidence_text TEXT,
    occurred_on TEXT NOT NULL CHECK (length(occurred_on) = 10),
    amount_minor INTEGER NOT NULL CHECK (abs(amount_minor) <= 1000000000000000),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    base_amount_minor INTEGER NOT NULL CHECK (abs(base_amount_minor) <= 1000000000000000),
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    rate_ppm INTEGER NOT NULL CHECK (rate_ppm > 0 AND rate_ppm <= 1000000000000000),
    direction TEXT NOT NULL CHECK (direction IN ('expense', 'income', 'transfer')),
    merchant TEXT NOT NULL CHECK (length(merchant) > 0),
    merchant_normalized TEXT,
    category TEXT,
    channel TEXT,
    note TEXT,
    account_id TEXT REFERENCES accounts(id),
    confirmed_at TEXT NOT NULL,
    deleted_at TEXT,
    CHECK (source_draft_id IS NULL OR source_id IS NOT NULL),
    CHECK ((source_id IS NULL AND evidence_text IS NULL) OR (source_id IS NOT NULL AND evidence_text IS NOT NULL))
);

CREATE INDEX transactions_occurred_on ON transactions(occurred_on);

CREATE TABLE audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    actor TEXT NOT NULL CHECK (actor IN ('agent', 'human', 'system')),
    at TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (length(entity_type) > 0),
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create', 'update', 'confirm', 'discard', 'void', 'delete')),
    before_json TEXT CHECK (before_json IS NULL OR json_valid(before_json)),
    after_json TEXT CHECK (after_json IS NULL OR json_valid(after_json)),
    agent_session_id TEXT,
    CHECK ((actor = 'agent' AND agent_session_id IS NOT NULL) OR actor <> 'agent')
);

CREATE TRIGGER sources_latest_attempt_insert_guard
BEFORE INSERT ON sources
WHEN NEW.latest_attempt_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM parse_attempts
        WHERE id = NEW.latest_attempt_id AND source_id = NEW.id
    ) THEN RAISE(ABORT, 'data.latest_attempt_source_mismatch') END;
END;

CREATE TRIGGER sources_latest_attempt_update_guard
BEFORE UPDATE OF latest_attempt_id ON sources
WHEN NEW.latest_attempt_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM parse_attempts
        WHERE id = NEW.latest_attempt_id AND source_id = NEW.id
    ) THEN RAISE(ABORT, 'data.latest_attempt_source_mismatch') END;
END;

CREATE TRIGGER parse_attempt_source_immutable
BEFORE UPDATE OF source_id ON parse_attempts
BEGIN
    SELECT RAISE(ABORT, 'data.attempt_source_immutable');
END;

CREATE TRIGGER draft_source_guard_insert
BEFORE INSERT ON draft_transactions
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM parse_attempts
        WHERE id = NEW.attempt_id AND source_id = NEW.source_id
    ) THEN RAISE(ABORT, 'data.draft_attempt_source_mismatch') END;
    SELECT CASE WHEN (
        SELECT kind FROM sources WHERE id = NEW.source_id
    ) = 'utterance' AND (NEW.evidence_span_start IS NULL OR NEW.evidence_span_end IS NULL)
    THEN RAISE(ABORT, 'data.utterance_span_required') END;
    SELECT CASE WHEN (
        SELECT kind FROM sources WHERE id = NEW.source_id
    ) = 'file' AND (NEW.evidence_span_start IS NOT NULL OR NEW.evidence_span_end IS NOT NULL)
    THEN RAISE(ABORT, 'data.file_span_forbidden') END;
END;

CREATE TRIGGER draft_source_guard_update
BEFORE UPDATE OF source_id, attempt_id, evidence_span_start, evidence_span_end ON draft_transactions
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM parse_attempts
        WHERE id = NEW.attempt_id AND source_id = NEW.source_id
    ) THEN RAISE(ABORT, 'data.draft_attempt_source_mismatch') END;
    SELECT CASE WHEN (
        SELECT kind FROM sources WHERE id = NEW.source_id
    ) = 'utterance' AND (NEW.evidence_span_start IS NULL OR NEW.evidence_span_end IS NULL)
    THEN RAISE(ABORT, 'data.utterance_span_required') END;
    SELECT CASE WHEN (
        SELECT kind FROM sources WHERE id = NEW.source_id
    ) = 'file' AND (NEW.evidence_span_start IS NOT NULL OR NEW.evidence_span_end IS NOT NULL)
    THEN RAISE(ABORT, 'data.file_span_forbidden') END;
END;

CREATE TRIGGER drafted_json_immutable
BEFORE UPDATE OF drafted_json ON draft_transactions
BEGIN
    SELECT RAISE(ABORT, 'data.drafted_json_immutable');
END;

CREATE TRIGGER audit_log_append_only_update
BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'data.audit_append_only');
END;

CREATE TRIGGER audit_log_append_only_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'data.audit_append_only');
END;

PRAGMA user_version = 1;
