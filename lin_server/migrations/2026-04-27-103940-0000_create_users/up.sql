-- -------------------------
-- https://dbdiagram.io/d --
-- Check out this tool    --
-- -------------------------

CREATE TABLE IF NOT EXISTS users (
    id BIGINT PRIMARY KEY,
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    phone TEXT NULL,
    displayed_name TEXT NULL,
    profile_img TEXT NULL,
    bio TEXT NULL,
    registred_at TIMESTAMP NOT NULL
)

