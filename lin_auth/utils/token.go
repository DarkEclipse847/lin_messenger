package utils

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"os"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
)

const AccessTokenTTL = 15 * time.Minute
const RefreshTokenTTL = 7 * 24 * time.Hour

// UUID claims reject legacy numeric identities during the migration cutover.
type TokenClaims struct {
	UserID    uuid.UUID `json:"user_id"`
	TokenType string    `json:"token_type"`
	jwt.RegisteredClaims
}

func SigningKey() ([]byte, error) {
	key := []byte(os.Getenv("JWT_SECRET"))
	if len(key) < 32 {
		return nil, errors.New("JWT_SECRET must contain at least 32 bytes")
	}
	return key, nil
}

func generateToken(id uuid.UUID, expires int64, tokenType string) (string, error) {
	key, err := SigningKey()
	if err != nil {
		return "", err
	}
	if id == uuid.Nil {
		return "", errors.New("invalid user ID")
	}
	var nonce [32]byte
	if _, err := rand.Read(nonce[:]); err != nil {
		return "", err
	}
	claims := TokenClaims{
		UserID: id, TokenType: tokenType,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Unix(expires, 0)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			ID:        hex.EncodeToString(nonce[:]),
		},
	}
	return jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString(key)
}

func GenerateToken(id uuid.UUID, expires int64) (string, error) {
	return generateToken(id, expires, "access")
}

// ValidateToken accepts access tokens only. Refresh tokens have a separate purpose.
func ValidateToken(token string) (*jwt.Token, error) {
	key, err := SigningKey()
	if err != nil {
		return nil, err
	}
	claims := &TokenClaims{}
	result, err := jwt.ParseWithClaims(token, claims, func(t *jwt.Token) (any, error) {
		return key, nil
	}, jwt.WithValidMethods([]string{"HS256"}), jwt.WithExpirationRequired(), jwt.WithIssuedAt())
	if err != nil {
		return nil, err
	}
	if !result.Valid || claims.UserID == uuid.Nil || claims.TokenType != "access" || claims.ID == "" {
		return nil, errors.New("invalid access token claims")
	}
	return result, nil
}

func GenerateTokenPair(id uuid.UUID) (string, string, error) {
	now := time.Now()
	access, err := generateToken(id, now.Add(AccessTokenTTL).Unix(), "access")
	if err != nil {
		return "", "", err
	}
	refresh, err := generateToken(id, now.Add(RefreshTokenTTL).Unix(), "refresh")
	if err != nil {
		return "", "", err
	}
	return access, refresh, nil
}
