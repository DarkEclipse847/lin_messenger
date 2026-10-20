-- The same mapping must be used in both independently owned databases.
-- Reserved legacy namespace; new identities are random UUIDv4 values.
CREATE OR REPLACE FUNCTION pg_temp.lin_user_uuid(old_id bigint) RETURNS uuid
LANGUAGE SQL IMMUTABLE STRICT AS $$
    SELECT (substr(h, 1, 8) || '-' || substr(h, 9, 4) || '-3' ||
            substr(h, 14, 3) || '-8' || substr(h, 18, 3) || '-' ||
            substr(h, 21, 12))::uuid
    FROM (SELECT md5('lin:legacy-user:' || old_id::text) AS h) AS digest
$$;
DO $$
DECLARE key record;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns
               WHERE table_schema = current_schema() AND table_name = 'users'
                 AND column_name = 'id' AND data_type = 'bigint') THEN
        LOCK TABLE users, auth_creds IN ACCESS EXCLUSIVE MODE;
        CREATE TEMP TABLE lin_user_foreign_keys ON COMMIT DROP AS
            SELECT conrelid::regclass AS tbl, conname, pg_get_constraintdef(oid) AS definition
            FROM pg_constraint WHERE contype = 'f' AND confrelid = 'users'::regclass;
        FOR key IN SELECT * FROM lin_user_foreign_keys LOOP
            EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', key.tbl, key.conname);
        END LOOP;
        ALTER TABLE users ALTER COLUMN id DROP DEFAULT;
        ALTER TABLE users ALTER COLUMN id TYPE uuid USING pg_temp.lin_user_uuid(id);
        ALTER TABLE auth_creds ALTER COLUMN user_id TYPE uuid USING pg_temp.lin_user_uuid(user_id);
        -- Existing JWTs carry integer identities and cannot survive this cutover.
        UPDATE auth_creds SET refresh_token_hash = '', access_token_hash = '';
        FOR key IN SELECT * FROM lin_user_foreign_keys LOOP
            EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s', key.tbl, key.conname, key.definition);
        END LOOP;
    END IF;
END $$;
