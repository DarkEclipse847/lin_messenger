package models

import "github.com/google/uuid"

// Id unique??
// Name unique??

// !!! INTERACTION TYPE IS RATHER "insert" or "sync"
// !!! sync is used to check integrity with the db
// !!! insert is used to register a new user
// !!! For now there only will be an "insert" type
type User struct {
	Id               uuid.UUID `json:"id" gorm:"type:uuid;primaryKey"`
	Interaction_Type string    `json:"interaction_type" gorm:"-"`
	Username         string    `json:"username" gorm:"unique"`
	Email            string    `json:"email" gorm:"unique"`
	Password         string    `json:"-"`
}

type AuthCreds struct {
	Id               int64     `json:"-" gorm:"primaryKey;autoIncrement"`
	UserID           uuid.UUID `json:"-" gorm:"type:uuid;uniqueIndex:user_refresh_idx"`
	AccessTokenHash  string    `json:"-" gorm:"size:128"`
	RefreshTokenHash string    `json:"-" gorm:"size:128"`
}
