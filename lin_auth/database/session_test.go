package database

import (
	"errors"
	"os"
	"testing"

	"github.com/google/uuid"
	"github.com/lin_auth/models"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

func TestSessionRotationAndRevocation(t *testing.T) {
	url := os.Getenv("LIN_TEST_DATABASE_URL")
	if url == "" {
		t.Skip("set LIN_TEST_DATABASE_URL to a disposable PostgreSQL database")
	}
	db, err := gorm.Open(postgres.Open(url), &gorm.Config{})
	if err != nil {
		t.Fatal(err)
	}
	sqlDB, _ := db.DB()
	defer sqlDB.Close()
	tx := db.Begin()
	if tx.Error != nil {
		t.Fatal(tx.Error)
	}
	defer tx.Rollback()
	schema := "session_test_" + uuid.New().String()[:8]
	if err := tx.Exec("CREATE SCHEMA " + schema + "; SET LOCAL search_path TO " + schema).Error; err != nil {
		t.Fatal(err)
	}
	if err := tx.AutoMigrate(&models.User{}, &models.AuthCreds{}); err != nil {
		t.Fatal(err)
	}
	user, err := CreateUser(tx, "alice", "alice@example.test", "unused")
	if err != nil {
		t.Fatal(err)
	}
	called := false
	update := func() error { called = true; return nil }
	if err := ChangeSession(tx, user.Id, "", "first", update); err != nil {
		t.Fatal(err)
	}
	if err := ChangeSession(tx, user.Id, "first", "second", update); err != nil {
		t.Fatal(err)
	}
	called = false
	if err := ChangeSession(tx, user.Id, "first", "replay", update); !errors.Is(err, ErrInvalidSession) || called {
		t.Fatal("replay accepted")
	}
	offline := errors.New("offline")
	if err := ChangeSession(tx, user.Id, "second", "third", func() error { return offline }); !errors.Is(err, offline) {
		t.Fatal(err)
	}
	// A failed Redis write must roll back rotation, allowing the current token to retry.
	if err := ChangeSession(tx, user.Id, "second", "", update); err != nil {
		t.Fatal(err)
	}
	called = false
	if err := ChangeSession(tx, user.Id, "second", "resurrected", update); !errors.Is(err, ErrInvalidSession) || called {
		t.Fatal("logged out session accepted")
	}
	if err := ChangeSession(tx, user.Id, "", "login", update); err != nil {
		t.Fatal(err)
	}
	called = false
	if err := ChangeSession(tx, user.Id, "second", "", update); !errors.Is(err, ErrInvalidSession) || called {
		t.Fatal("stale logout revoked new login")
	}
}
