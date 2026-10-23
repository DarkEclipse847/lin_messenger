-- The same mapping must be used in both independently owned databases.
-- Reserved legacy namespace; new identities are random UUIDv4 values.
CREATE OR REPLACE FUNCTION pg_temp.lin_user_uuid(old_id bigint) RETURNS uuid
LANGUAGE SQL IMMUTABLE STRICT AS $$
    SELECT (substr(h, 1, 8) || '-' || substr(h, 9, 4) || '-3' ||
            substr(h, 14, 3) || '-8' || substr(h, 18, 3) || '-' ||
            substr(h, 21, 12))::uuid
    FROM (SELECT md5('lin:legacy-user:' || old_id::text) AS h) AS digest
$$;
LOCK TABLE users, chats, messages IN ACCESS EXCLUSIVE MODE;
CREATE TEMP TABLE lin_user_foreign_keys ON COMMIT DROP AS
    SELECT conrelid::regclass AS tbl, conname, pg_get_constraintdef(oid) AS definition
    FROM pg_constraint WHERE contype = 'f' AND confrelid = 'users'::regclass;
DO $$
DECLARE key record;
BEGIN
    FOR key IN SELECT * FROM lin_user_foreign_keys LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', key.tbl, key.conname);
    END LOOP;
END $$;
ALTER TABLE users ALTER COLUMN id DROP DEFAULT;
ALTER TABLE users ALTER COLUMN id TYPE uuid USING pg_temp.lin_user_uuid(id);
ALTER TABLE chats ALTER COLUMN admin TYPE uuid USING pg_temp.lin_user_uuid(admin);
ALTER TABLE messages ALTER COLUMN sender_id TYPE uuid USING pg_temp.lin_user_uuid(sender_id);
DO $$
DECLARE key record;
BEGIN
    FOR key IN SELECT * FROM lin_user_foreign_keys LOOP
        EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s', key.tbl, key.conname, key.definition);
    END LOOP;
END $$;
