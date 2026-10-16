package main

import (
	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/database"
	r "github.com/lin_auth/redis"
	"github.com/lin_auth/routes"
	"github.com/lin_auth/utils"
	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
)

func main() {
	if _, err := utils.SigningKey(); err != nil {
		log.Fatal().Msg(err.Error())
	}
	if err := r.Initialize(); err != nil {
		log.Fatal().Msg(err.Error())
	}
	db := database.Connect()

	zerolog.TimeFieldFormat = zerolog.TimeFormatUnix

	app := fiber.New(fiber.Config{BodyLimit: 16 * 1024})
	routes.Setup(app, db)
	if err := app.Listen(":8000"); err != nil {
		log.Fatal().Msg("Auth HTTP server stopped")
	}
}
