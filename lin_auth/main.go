package main

import (
	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/database"
	"github.com/lin_auth/routes"
	"github.com/rs/zerolog"
)

func main() {
	//db connection
	database.Connect()

	//Logging init
	zerolog.TimeFieldFormat = zerolog.TimeFormatUnix;

	//Fiber init
	app := fiber.New();
	routes.Setup(app)
	app.Listen(":8000")
}