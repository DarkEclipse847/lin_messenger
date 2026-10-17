package database

import (
	_ "embed"
	"gorm.io/gorm"
)

//go:embed migrations/001_user_uuid.sql
var userUUIDMigration string

// Run before AutoMigrate: PostgreSQL cannot cast legacy BIGINT identities to UUID.
func migrateUserIDs(db *gorm.DB) error {
	return db.Transaction(func(tx *gorm.DB) error {
		return tx.Exec(userUUIDMigration).Error
	})
}
