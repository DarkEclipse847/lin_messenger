package controllers

import (
	"errors"
	"net/mail"
	"net/url"
	"regexp"
	"strings"
	"time"

	"github.com/gofiber/fiber/v3"
	"github.com/google/uuid"
	"github.com/lin_auth/database"
	"github.com/lin_auth/models"
	r "github.com/lin_auth/redis"
	"github.com/lin_auth/utils"
	goredis "github.com/redis/go-redis/v9"
	"github.com/rs/zerolog/log"
	"gorm.io/gorm"
)

var usernamePattern = regexp.MustCompile(`^[A-Za-z0-9_]{3,32}$`)

type credentials struct {
	Username string `json:"username" form:"username"`
	Email    string `json:"email" form:"email"`
	Password string `json:"password" form:"password"`
}

func readCredentials(c fiber.Ctx) (credentials, error) {
	var data credentials
	if err := c.Bind().Body(&data); err != nil {
		return data, fiber.ErrBadRequest
	}
	data.Username = strings.TrimSpace(data.Username)
	data.Email = strings.TrimSpace(data.Email)
	if len(data.Username) > 32 || len(data.Email) > 254 || len(data.Password) == 0 || len(data.Password) > 1024 {
		return data, fiber.ErrBadRequest
	}
	return data, nil
}

func extractAccessToken(c fiber.Ctx) string {
	if header := c.Get("Authorization"); header != "" {
		parts := strings.Fields(header)
		if len(parts) == 2 && strings.EqualFold(parts[0], "bearer") {
			return parts[1]
		}
		// An explicit invalid credential must not silently fall back to a cookie.
		return ""
	}
	return strings.TrimSpace(c.Cookies("access_token"))
}

func parseUserIDFromToken(token string) (uuid.UUID, error) {
	parsed, err := utils.ValidateToken(token)
	if err != nil {
		return uuid.Nil, err
	}
	claims, ok := parsed.Claims.(*utils.TokenClaims)
	if !ok {
		return uuid.Nil, errors.New("invalid token claims")
	}
	return claims.UserID, nil
}

func Validate(c fiber.Ctx, db *gorm.DB) error {
	return validateSession(c, r.GetAccessToken)
}

func validateSession(c fiber.Ctx, lookup func(uuid.UUID) (string, error)) error {
	c.Set("Cache-Control", "no-store")
	token := extractAccessToken(c)
	if token == "" {
		return fiber.ErrUnauthorized
	}
	userID, err := parseUserIDFromToken(token)
	if err != nil {
		return fiber.ErrUnauthorized
	}
	storedToken, err := lookup(userID)
	if errors.Is(err, goredis.Nil) {
		return fiber.ErrUnauthorized
	}
	if err != nil {
		log.Error().Msg("Session store unavailable")
		return fiber.ErrServiceUnavailable
	}
	if storedToken != token {
		return fiber.ErrUnauthorized
	}
	c.Set("X-User-Id", userID.String())
	return c.SendStatus(fiber.StatusOK)
}

func Register(c fiber.Ctx, db *gorm.DB) error {
	c.Set("Cache-Control", "no-store")
	data, err := readCredentials(c)
	if err != nil {
		return err
	}
	address, err := mail.ParseAddress(data.Email)
	if !usernamePattern.MatchString(data.Username) || err != nil || address.Address != data.Email || len(data.Password) < 8 {
		return fiber.ErrBadRequest
	}
	password, err := utils.GenerateHashFromPasswd(data.Password)
	if err != nil {
		return fiber.ErrInternalServerError
	}
	user, err := database.CreateUser(db, data.Username, data.Email, password)
	if errors.Is(err, gorm.ErrDuplicatedKey) {
		return fiber.ErrConflict
	}
	if err != nil {
		log.Error().Msg("Failed to create user")
		return fiber.ErrInternalServerError
	}
	if err := rabbitSend("REGISTRATION_REQ", &user); err != nil {
		// Publishing is still best effort until an auth-owned outbox is introduced.
		log.Error().Msg("Registration profile publication failed; reconciliation required")
	}
	return issueSession(c, db, user)
}

func Login(c fiber.Ctx, db *gorm.DB) error {
	c.Set("Cache-Control", "no-store")
	data, err := readCredentials(c)
	if err != nil {
		return err
	}
	// Exactly one identifier avoids OR queries matching an unrelated empty field
	// or ambiguously selecting between two different users.
	if (data.Email == "") == (data.Username == "") {
		return fiber.ErrBadRequest
	}
	var user models.User
	query := db.Where("username = ?", data.Username)
	if data.Email != "" {
		query = db.Where("email = ?", data.Email)
	}
	if err := query.First(&user).Error; err != nil {
		if errors.Is(err, gorm.ErrRecordNotFound) {
			return fiber.ErrUnauthorized
		}
		log.Error().Msg("Failed to read user")
		return fiber.ErrServiceUnavailable
	}
	if !utils.ComparePassword(user.Password, data.Password) {
		return fiber.ErrUnauthorized
	}
	return issueSession(c, db, user)
}

func issueSession(c fiber.Ctx, db *gorm.DB, user models.User) error {
	access, refresh, err := utils.GenerateTokenPair(user.Id)
	if err != nil {
		return fiber.ErrInternalServerError
	}
	if err := database.CreateAuth(db, user.Id, refresh); err != nil {
		log.Error().Msg("Failed to store refresh session")
		return fiber.ErrServiceUnavailable
	}
	if err := r.StoreAccessToken(user.Id, access, utils.AccessTokenTTL); err != nil {
		log.Error().Msg("Failed to store access session")
		return fiber.ErrServiceUnavailable
	}
	setSessionCookies(c, access, refresh)
	// A POST must not produce a permanently cached redirect. Escape legacy usernames.
	return c.Redirect().Status(fiber.StatusSeeOther).To("/" + url.PathEscape(user.Username))
}

func setSessionCookies(c fiber.Ctx, access, refresh string) {
	now := time.Now()
	for _, token := range []struct {
		name, value string
		ttl         time.Duration
	}{
		{"access_token", access, utils.AccessTokenTTL},
		{"refresh_token", refresh, utils.RefreshTokenTTL},
	} {
		c.Cookie(&fiber.Cookie{
			Name: token.name, Value: token.value, Expires: now.Add(token.ttl),
			HTTPOnly: true, Secure: true, SameSite: "Lax", Path: "/",
		})
	}
}
