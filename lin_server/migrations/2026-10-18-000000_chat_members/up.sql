CREATE TABLE chat_members (
    chat_id BIGINT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (chat_id, user_id)
);
CREATE INDEX chat_members_user_id_idx ON chat_members(user_id);
-- Only existing administrators have provable access to legacy chats.
INSERT INTO chat_members(chat_id, user_id)
SELECT id, admin FROM chats WHERE admin IS NOT NULL;
