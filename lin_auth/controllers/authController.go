// ~~TODO: Parse google creds, create google oauth controller~~
// https://www.youtube.com/watch?v=tgO_ADSvY1I
//
// OKAY FUCKING NEVERMIND
//
//////////////
// FUCK RKN //
//////////////

package controllers

import (
	"errors"
	"strconv"
	"strings"
	"time"

	"github.com/gofiber/fiber/v3"
	"github.com/golang-jwt/jwt/v5"
	"github.com/lin_auth/database"
	"github.com/lin_auth/models"
	r "github.com/lin_auth/redis"
	"github.com/lin_auth/utils"
	"github.com/rs/zerolog/log"
	"gorm.io/gorm"
)


func extractAccessToken(c fiber.Ctx) string {
	authHeader := c.Get("Authorization")
	if authHeader != "" {
		parts := strings.Fields(authHeader)
		if len(parts) == 2 && strings.ToLower(parts[0]) == "bearer" {
			return parts[1]
		}
	}

	if cookie := c.Cookies("access_token"); cookie != "" {
		return cookie
	}

	return ""
}

func parseUserIDFromToken(token string) (int64, error) {
	parsed, err := utils.ValidateToken(token)
	if err != nil {
		return 0, err
	}

	claims, ok := parsed.Claims.(jwt.MapClaims)
	if !ok {
		return 0, errors.New("invalid token claims")
	}

	userIDClaim, ok := claims["user_id"]
	if !ok {
		return 0, errors.New("missing user_id claim")
	}

	switch v := userIDClaim.(type) {
	case float64:
		return int64(v), nil
	case int64:
		return v, nil
	case int:
		return int64(v), nil
	case string:
		return strconv.ParseInt(v, 10, 64)
	default:
		return 0, errors.New("invalid user_id claim type")
	}
}

func Validate(c fiber.Ctx, db *gorm.DB) error {
	token := extractAccessToken(c)
	if token == "" {
		return c.Status(fiber.StatusUnauthorized).SendString("missing token")
	}

	userID, err := parseUserIDFromToken(token)
	if err != nil {
		log.Error().Err(err).Msg("failed to parse access token")
		return c.Status(fiber.StatusUnauthorized).SendString("invalid token")
	}

	storedToken, err := r.GetAccessToken(userID)
	if err != nil {
		log.Error().Err(err).Msg("failed to read access token from redis")
		return c.Status(fiber.StatusUnauthorized).SendString("session expired")
	}

	if storedToken != token {
		log.Error().Msg("access token mismatch")
		return c.Status(fiber.StatusUnauthorized).SendString("invalid token")
	}

	c.Set("X-User-Id", strconv.FormatInt(userID, 10))
	return c.SendStatus(fiber.StatusOK)
}

func Register(c fiber.Ctx, db *gorm.DB) error {
	var data map[string]string

	if err := c.Bind().Body(&data); err != nil {
		log.Error().Msg(err.Error())
		return err
	}

	password := utils.GenerateHashFromPasswd(data["password"])

	user, err := database.CreateUser(db, data["username"], data["email"], password)
	if err != nil{
		log.Error().Msg(err.Error())
		return err
	}

	rabbitSend("REGISTRATION_REQ", &user)

	access_token, refresh_token, err := utils.GenerateTokenPair(user.Id)
	if err != nil {
		log.Error().Msg(err.Error())
		return err
	}

	// Persist refresh token hash in Postgres and store access token in Redis.
	if err := database.CreateAuth(db, user.Id, refresh_token); err != nil {
		log.Error().Msgf("failed to create auth record: %v", err)
		return err
	}

	if err := r.StoreAccessToken(user.Id, access_token, 15*time.Minute); err != nil {
		log.Error().Msgf("failed to store access token in redis: %v", err)
		return err
	}

	// Set HTTP-only cookies for browser clients. Access token TTL matches Redis TTL.
	c.Cookie(&fiber.Cookie{
		Name: "access_token",
		Value: access_token,
		Expires: time.Now().Add(15*time.Minute),
		HTTPOnly: true, // Like readonly but for cookies
		Secure: true, //HTTPS only
		Path: "/", //Website-wide cookie usage
	})
	c.Cookie(&fiber.Cookie{
		Name: "refresh_token",
		Value: refresh_token,
		HTTPOnly: true, // Like readonly but for cookies
		Secure: true, //HTTPS only
		Path: "/", //Website-wide cookie usage
	})
	

    //REDIRECT USER TO lin.wtf/{id}
	return c.Redirect().Status(fiber.StatusMovedPermanently).To("/"+user.Username)
}

func Login(c fiber.Ctx, db *gorm.DB) error {
	var data map[string]string

	if err := c.Bind().Body(&data); err != nil {
		log.Error().Msg(err.Error())
		return err
	}

	var user models.User
	if err := db.Where("email = ? OR username = ?", data["email"], data["username"]).First(&user).Error; err != nil {
		log.Error().Msgf("user not found: %v", err)
		return fiber.ErrUnauthorized
	}

	if !utils.ComparePassword(user.Password, data["password"]) {
		log.Error().Msg("invalid password")
		return fiber.ErrUnauthorized
	}

	access_token, refresh_token, err := utils.GenerateTokenPair(user.Id)
	if err != nil {
		log.Error().Msg(err.Error())
		return err
	}

	if err := database.CreateAuth(db, user.Id, refresh_token); err != nil {
		log.Error().Msgf("failed to update auth record: %v", err)
		return err
	}

	if err := r.StoreAccessToken(user.Id, access_token, 15*time.Minute); err != nil {
		log.Error().Msgf("failed to store access token in redis: %v", err)
		return err
	}

	c.Cookie(&fiber.Cookie{
		Name: "access_token",
		Value: access_token,
		Expires: time.Now().Add(15 * time.Minute),
		HTTPOnly: true,
		Secure: true,
		Path: "/",
	})
	c.Cookie(&fiber.Cookie{
		Name: "refresh_token",
		Value: refresh_token,
		HTTPOnly: true,
		Secure: true,
		Path: "/",
	})

	return c.Redirect().Status(fiber.StatusMovedPermanently).To("/" + user.Username)
}
