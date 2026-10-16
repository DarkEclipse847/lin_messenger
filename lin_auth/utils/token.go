package utils

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"os"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

const AccessTokenTTL = 15 * time.Minute
const RefreshTokenTTL = 7 * 24 * time.Hour

// Keep IDs as integers throughout decoding: float64 loses precision above 2^53.
type TokenClaims struct {
	UserID    int64  `json:"user_id"`
	TokenType string `json:"token_type"`
	jwt.RegisteredClaims
}

func SigningKey() ([]byte, error) {
	key := []byte(os.Getenv("JWT_SECRET"))
	if len(key) < 32 {
		return nil, errors.New("JWT_SECRET must contain at least 32 bytes")
	}
	return key, nil
}

func generateToken(id int64, expires int64, tokenType string) (string, error) {
	key, err := SigningKey()
	if err != nil {
		return "", err
	}
	if id <= 0 {
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

func GenerateToken(id int64, expires int64) (string, error) {
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
	if !result.Valid || claims.UserID <= 0 || claims.TokenType != "access" || claims.ID == "" {
		return nil, errors.New("invalid access token claims")
	}
	return result, nil
}

func GenerateTokenPair(id int64) (string, string, error) {
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
