package database

import (
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"

	"github.com/google/uuid"
	"github.com/lin_auth/models"
	"gorm.io/gorm"
	"gorm.io/gorm/clause"
)

func CreateUser(db *gorm.DB, username string, email string, password string) (models.User, error) {
	id, err := uuid.NewRandom()
	if err != nil {
		return models.User{}, err
	}
	user := models.User{
		Id:               id,
		Interaction_Type: "insert",
		Username:         username,
		Email:            email,
		Password:         password,
	}
	err = db.Create(&user).Error
	return user, err
}

func CreateAuth(db *gorm.DB, id uuid.UUID, refreshToken string) error {
	hash := sha256.Sum256([]byte(refreshToken))
	auth := models.AuthCreds{
		UserID:           id,
		RefreshTokenHash: hex.EncodeToString(hash[:]),
	}
	// Access tokens are stored only in Redis.
	return db.Clauses(clause.OnConflict{
		Columns:   []clause.Column{{Name: "user_id"}},
		DoUpdates: clause.AssignmentColumns([]string{"refresh_token_hash"}),
	}).Omit("AccessTokenHash").Create(&auth).Error
}

// ErrInvalidSession means the presented refresh credential is no longer current.
var ErrInvalidSession = errors.New("invalid session")

// ChangeSession serializes login, refresh and logout on the user row, including
// their Redis writes. An empty current token is reserved for authenticated login.
// Redis and PostgreSQL cannot commit atomically; callers must report failures.
func ChangeSession(db *gorm.DB, id uuid.UUID, current, next string, updateAccess func() error) error {
	return db.Transaction(func(tx *gorm.DB) error {
		var user models.User
		if err := tx.Clauses(clause.Locking{Strength: "UPDATE"}).First(&user, "id = ?", id).Error; err != nil {
			if errors.Is(err, gorm.ErrRecordNotFound) {
				return ErrInvalidSession
			}
			return err
		}
		if current != "" {
			hash := sha256.Sum256([]byte(current))
			var auth models.AuthCreds
			err := tx.Where("user_id = ?", id).First(&auth).Error
			if errors.Is(err, gorm.ErrRecordNotFound) {
				return ErrInvalidSession
			}
			if err != nil {
				return err
			}
			if subtle.ConstantTimeCompare([]byte(auth.RefreshTokenHash), []byte(hex.EncodeToString(hash[:]))) != 1 {
				return ErrInvalidSession
			}
		}
		if next == "" {
			if err := tx.Where("user_id = ?", id).Delete(&models.AuthCreds{}).Error; err != nil {
				return err
			}
		} else if err := CreateAuth(tx, id, next); err != nil {
			return err
		}
		return updateAccess()
	})
}
