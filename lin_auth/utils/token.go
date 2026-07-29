// TODO: Add rotating JWT secret key
package utils

import (
	"errors"
	"os"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

func GenerateToken(id int64, lifetime int64) (string, error) {
	token, err:= jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{
		"user_id": id,
		"exp": lifetime,
	}).SignedString([]byte(os.Getenv("JWT_SECRET")))

	if err != nil {
		return "", err
	}

	return token, err;
}

func ValidateToken(token string) (*jwt.Token, error) {
    result, err := jwt.Parse(token, func(t *jwt.Token) (interface{}, error) {
        if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
            return nil, errors.New("unexpected signing method")
        }
        return []byte(os.Getenv("JWT_SECRET")), nil
    })
    if err != nil {
        return nil, err
    }
    if !result.Valid {
        return nil, errors.New("invalid token")
    }
    return result, nil
}

func GenerateTokenPair(id int64) (string, string, error) {
	access_time := time.Now().Add(15*time.Minute).Unix()
	refresh_time := time.Now().Add(168*time.Hour).Unix()

	access_token, err := GenerateToken(id, access_time)
	if err != nil {
		return "", "", err
	}
	refresh_token, err := GenerateToken(id, refresh_time)
	if err != nil {
		return "", "", err
	}

	return access_token, refresh_token, err
}

