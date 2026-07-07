package database

import (
	"context"

	"github.com/lin_auth/models"
	"gorm.io/gorm"
)


func CreateUser(db *gorm.DB, username string, email string, password string) (models.User, error){
	user := models.User{
		Interaction_Type: "insert", 
		Username: username,
		Email: email,
		Password: password,
	}
	ctx := context.Background()
	err := gorm.G[models.User](db).Create(ctx, &user)
	//err := db.Clauses(clause.Returning{clause.Column{{Name: "id"}}})

	return user, err
}

func CreateAuth(db *gorm.DB, id int64, refresh_token string, access_token string){
	
}