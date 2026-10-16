package utils

import "github.com/alexedwards/argon2id"

func GenerateHashFromPasswd(password string) (string, error) {
	return argon2id.CreateHash(password, &argon2id.Params{
		Memory: 64 * 1024, Iterations: 4, Parallelism: 1, SaltLength: 16, KeyLength: 32,
	})
}

func ComparePassword(hashedPassword, password string) bool {
	result, err := argon2id.ComparePasswordAndHash(password, hashedPassword)
	return err == nil && result
}
