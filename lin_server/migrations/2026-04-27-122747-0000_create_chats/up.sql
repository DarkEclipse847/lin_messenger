-- --------------
-- CHATS TABLE --
-- --------------


CREATE TABLE IF NOT EXISTS chats (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    chat_img TEXT NULL,
    admin BIGINT REFERENCES users(id),
    bio TEXT NULL,
    created_at TIMESTAMP NOT NULL
)
