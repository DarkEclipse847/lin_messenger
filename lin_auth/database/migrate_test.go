package database

import (
	"os"
	"testing"

	"github.com/google/uuid"
	"github.com/lin_auth/models"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

// Uses an isolated schema and rolls back all changes, including the schema.
func TestUserUUIDMigration(t *testing.T) {
	url := os.Getenv("LIN_TEST_DATABASE_URL")
	if url == "" {
		t.Skip("set LIN_TEST_DATABASE_URL to a disposable PostgreSQL database")
	}
	db, err := gorm.Open(postgres.Open(url), &gorm.Config{})
	if err != nil {
		t.Fatal(err)
	}
	sqlDB, err := db.DB()
	if err != nil {
		t.Fatal(err)
	}
	defer sqlDB.Close()
	tx := db.Begin()
	if tx.Error != nil {
		t.Fatal(tx.Error)
	}
	defer tx.Rollback()
	schema := "uuid_test_" + uuid.New().String()[:8]
	if err := tx.Exec("CREATE SCHEMA " + schema + "; SET LOCAL search_path TO " + schema).Error; err != nil {
		t.Fatal(err)
	}
	if err := tx.Exec(`CREATE TABLE users (id bigserial PRIMARY KEY, username text UNIQUE, email text UNIQUE, password text);
 CREATE TABLE auth_creds (id bigserial PRIMARY KEY, user_id bigint REFERENCES users(id), access_token_hash varchar(128), refresh_token_hash varchar(128));
 CREATE UNIQUE INDEX user_refresh_idx ON auth_creds(user_id);
 INSERT INTO users(id, username, email, password) VALUES (42,'alice','alice@example.test','preserved'),(9223372036854775807,'bob','bob@example.test','preserved');
 INSERT INTO auth_creds(user_id,refresh_token_hash) VALUES (42,'old-hash');`).Error; err != nil {
		t.Fatal(err)
	}
	for range 2 {
		if err := migrateUserIDs(tx); err != nil {
			t.Fatal(err)
		}
	}
	if err := tx.AutoMigrate(&models.User{}, &models.AuthCreds{}); err != nil {
		t.Fatal(err)
	}
	for name, id := range map[string]string{"alice": "016939df-e261-3cd7-8cd6-4ae01aa40c54", "bob": "96af6c64-b564-3e3d-831b-d2cbd5a82f28"} {
		var user models.User
		if err := tx.Where("username = ?", name).First(&user).Error; err != nil {
			t.Fatal(err)
		}
		if user.Id.String() != id || user.Password != "preserved" {
			t.Fatal("legacy identity or credentials changed")
		}
	}
	var session models.AuthCreds
	if err := tx.First(&session).Error; err != nil {
		t.Fatal(err)
	}
	if session.UserID.String() != "016939df-e261-3cd7-8cd6-4ae01aa40c54" || session.RefreshTokenHash != "" {
		t.Fatal("session not migrated/revoked")
	}
	user, err := CreateUser(tx, "charlie", "charlie@example.test", "hash")
	if err != nil {
		t.Fatal(err)
	}
	if user.Id == uuid.Nil || user.Id.Version() != 4 {
		t.Fatal("new user must receive random UUIDv4")
	}
	if err := CreateAuth(tx, user.Id, "refresh"); err != nil {
		t.Fatal(err)
	}
	if err := CreateAuth(tx, user.Id, "replacement"); err != nil {
		t.Fatal(err)
	}
	// Fresh installations must also work without a legacy users table.
	if err := tx.Exec("CREATE SCHEMA " + schema + "_fresh; SET LOCAL search_path TO " + schema + "_fresh").Error; err != nil {
		t.Fatal(err)
	}
	if err := migrateUserIDs(tx); err != nil {
		t.Fatal(err)
	}
	if err := tx.AutoMigrate(&models.User{}, &models.AuthCreds{}); err != nil {
		t.Fatal(err)
	}
	fresh, err := CreateUser(tx, "fresh", "fresh@example.test", "hash")
	if err != nil || fresh.Id.Version() != 4 {
		t.Fatalf("fresh UUID schema failed: %v", err)
	}

}
