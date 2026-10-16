package database

import (
	"log"
	"os"
	"time"

	"github.com/lin_auth/models"
	zlog "github.com/rs/zerolog/log"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
	"gorm.io/gorm/logger"
)

func Connect() *gorm.DB {
	connection, err := gorm.Open(postgres.Open(os.Getenv("DATABASE_URL")), &gorm.Config{
		TranslateError: true,
		Logger: logger.New(log.New(os.Stderr, "", log.LstdFlags), logger.Config{
			SlowThreshold: time.Second, LogLevel: logger.Warn, IgnoreRecordNotFoundError: true, ParameterizedQueries: true,
		}),
	})
	if err != nil {
		zlog.Fatal().Msg("Failed to connect to auth database")
	}

	if err := migrateUserIDs(connection); err != nil {
		zlog.Fatal().Msg("Failed to migrate user identities to UUID")
	}

	if err := connection.AutoMigrate(&models.User{}, &models.AuthCreds{}); err != nil {
		zlog.Fatal().Msg("Failed to migrate auth database")
	}

	return connection
}
