package main

import (
	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/database"
	"github.com/lin_auth/routes"
	"github.com/rs/zerolog"
)

func main() {
	db := database.Connect()

	zerolog.TimeFieldFormat = zerolog.TimeFormatUnix

	app := fiber.New()
	routes.Setup(app, db)
	app.Listen(":8000")
}
