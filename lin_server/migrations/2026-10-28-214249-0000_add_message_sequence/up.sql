-- Backfill deterministic chronological cursors; preserve all legacy rows, including NULL times.
ALTER TABLE messages ADD COLUMN sequence BIGINT;
WITH ordered AS (
    SELECT id, row_number() OVER (ORDER BY sent_at NULLS FIRST, id) AS seq FROM messages
)
UPDATE messages SET sequence = ordered.seq FROM ordered WHERE messages.id = ordered.id;
CREATE SEQUENCE message_sequence AS BIGINT;
SELECT setval('message_sequence', COALESCE(MAX(sequence), 0) + 1, false) FROM messages;
ALTER SEQUENCE message_sequence OWNED BY messages.sequence;
ALTER TABLE messages ALTER COLUMN sequence SET DEFAULT nextval('message_sequence');
ALTER TABLE messages ALTER COLUMN sequence SET NOT NULL;
CREATE UNIQUE INDEX messages_chat_sequence_idx ON messages(chat_id, sequence);
