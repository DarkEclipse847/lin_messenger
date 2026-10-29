-- Rolling back invalidates client cursors; clients must restart recovery from zero.
ALTER TABLE messages DROP COLUMN sequence;
