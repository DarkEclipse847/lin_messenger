// Add rotating JWT secret key
package utils

import (
	"os"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

func GenerateToken(id int64, lifetime int64) (string, error) {
	token, err:= jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{
		"user_id": id,
		"exp": lifetime,
	}).SignedString(os.Getenv("JWT_SECRET"))

	if err != nil {
		return "", err
	}

	return token, err;
}

func VerifyToken (token string) (bool, error){
	result, err := jwt.Parse(token, func(result *jwt.Token) (interface{}, error){
		return []byte(os.Getenv("JWT_SECRET")), nil
	})
	if err != nil{
		return false, err
	}
	return result.Valid, nil
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

