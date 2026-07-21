package database

import (
	"crypto/sha256"
	"encoding/hex"

	"github.com/lin_auth/models"
	"gorm.io/gorm"
	"gorm.io/gorm/clause"
)

func CreateUser(db *gorm.DB, username string, email string, password string) (models.User, error) {
	user := models.User{
		Interaction_Type: "insert",
		Username:         username,
		Email:            email,
		Password:         password,
	}
	err := db.Create(&user).Error
	return user, err
}

func CreateAuth(db *gorm.DB, id int64, refreshToken string) error {
	hash := sha256.Sum256([]byte(refreshToken))
	auth := models.AuthCreds{
		UserID:           id,
		RefreshTokenHash: hex.EncodeToString(hash[:]),
	}
	//Omitting Access token which will be stored in redis
	// also what the actual fuck i ve written here
	// TODO: Check docs on *gorm.DB.Clauses 
	return db.Clauses(clause.OnConflict{
		Columns:   []clause.Column{{Name: "user_id"}},
		DoUpdates: clause.AssignmentColumns([]string{"refresh_token_hash"}),
	}).Omit("AccessTokenHash").Create(&auth).Error
}
