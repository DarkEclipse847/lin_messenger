package models

// Id unique??
// Name unique??

// !!! INTERACTION TYPE IS RATHER "insert" or "sync"
// !!! sync is used to check integrity with the db
// !!! insert is used to register a new user
// !!! For now there only will be an "insert" type
type User struct {
	Id               int64  `json:"id" gorm:"primaryKey;autoincrement"`
	Interaction_Type string `json:"interaction_type" gorm:"-"`
	Username         string `json:"username" gorm:"unique"`
	Email            string `json:"email" gorm:"unique"`
	Password         string `json:"-"`
}

type AuthCreds struct {
	Id               int64  `json:"-" gorm:"primaryKey;autoIncrement"`
	UserID           int64  `json:"-" gorm:"uniqueIndex:user_refresh_idx"`
	AccessTokenHash string `json:"-" gorm:"size:128"`
	RefreshTokenHash string `json:"-" gorm:"size:128"`
}
