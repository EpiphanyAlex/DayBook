ALTER TABLE parse_attempts ADD COLUMN requested_model_mode TEXT
    CHECK (requested_model_mode IN ('auto', 'specific'));
ALTER TABLE parse_attempts ADD COLUMN requested_model_id TEXT;
CREATE TRIGGER parse_attempts_requested_model_insert
BEFORE INSERT ON parse_attempts
WHEN NOT (
    (NEW.requested_model_mode IS NULL AND NEW.requested_model_id IS NULL)
    OR (NEW.requested_model_mode = 'auto' AND NEW.requested_model_id IS NULL)
    OR (NEW.requested_model_mode = 'specific' AND NEW.requested_model_id IS NOT NULL AND length(NEW.requested_model_id) > 0)
)
BEGIN
    SELECT RAISE(ABORT, 'agent.invalid_model_selection');
END;
PRAGMA user_version = 2;
