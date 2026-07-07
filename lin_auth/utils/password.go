package utils

import (
	"github.com/rs/zerolog/log"

	"github.com/alexedwards/argon2id"
)

func GenerateHashFromPasswd(password string) string{
	params := &argon2id.Params{
		Memory:      64 * 1024,
		Iterations:  4,
		Parallelism: 1,
		SaltLength:  16,
		KeyLength:   32,
	}

	password, err := argon2id.CreateHash(password, params)
	if err != nil {
		log.Error().Msg(err.Error())
	}
	return password
}

func ComparePassword(hashedPassword string, password string) bool{
	result, err := argon2id.ComparePasswordAndHash(password, hashedPassword)
	if err != nil{
		log.Error().Msg(err.Error())
	}
	return result
}