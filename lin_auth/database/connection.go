package database

import (
	"os"

	"github.com/lin_auth/models"
	"github.com/rs/zerolog/log"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

func Connect() *gorm.DB {
	connection, err := gorm.Open(postgres.Open(os.Getenv("DATABASE_URL")), &gorm.Config{})
	if err != nil {
		log.Fatal().Msg(err.Error())
	}

	if err := connection.AutoMigrate(&models.User{}, &models.AuthCreds{}); err != nil {
		log.Fatal().Msg(err.Error())
	}

	return connection
}
