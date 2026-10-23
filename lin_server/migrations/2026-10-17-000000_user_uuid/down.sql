-- UUIDs created after cutover have no integer equivalent. Restore both backups.
DO $$ BEGIN RAISE EXCEPTION 'User UUID migration is irreversible; restore coordinated database backups'; END $$;
