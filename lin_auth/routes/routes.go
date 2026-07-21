package routes

import (
	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/controllers"
	"gorm.io/gorm"
)

func Setup(app *fiber.App, db *gorm.DB) {
	app.Post("/api/register", func(c fiber.Ctx) error {
		return controllers.Register(c, db)
	})

	app.Post("/api/login", func(c fiber.Ctx) error {
		return controllers.Login(c, db)
	})

	app.All("/api/auth/validate", func(c fiber.Ctx) error {
		return controllers.Validate(c, db)
	})
}
