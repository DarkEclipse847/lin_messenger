-- -----------------
-- MESSAGES TABLE --
-- -----------------

CREATE TABLE IF NOT EXISTS messages (
    id BIGINT PRIMARY KEY,
    chat_id BIGINT REFERENCES chats(id),
    sender_id BIGINT REFERENCES users(id),
    body TEXT,
    sent_at TIMESTAMP,
    attachment TEXT NULL
)
