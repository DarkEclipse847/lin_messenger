package redis

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/redis/go-redis/v9"
	"github.com/rs/zerolog/log"
)

// NewClient initializes a Redis client using a connection URL.
func NewClient() *redis.Client {
	redisURL := os.Getenv("REDIS_URL")
	if redisURL == "" {
		log.Error().Msg("Redis URL not found, using loopback to localhost")
		// Fallback to local default if the environment variable is missing
		redisURL = "redis://localhost:6379/0"
	}

	// ParseURL extracts Address, Password, and DB index automatically
	opts, err := redis.ParseURL(redisURL)
	if err != nil {
		// Log the error and fall back to empty/default options so the application doesn't crash instantly,
		// though connections will fail until the URL is fixed.
		// log.Printf("failed to parse redis url: %v", err)
		return redis.NewClient(&redis.Options{})
	}

	return redis.NewClient(opts)
}

// StoreAccessToken stores a token with a specific TTL.
// Optimization Note: It is highly recommended to reuse a single client instance 
// instead of creating and closing a new client connection on every function call.
func StoreAccessToken(userID int64, token string, ttl time.Duration) error {
	client := NewClient()
	defer client.Close()
	key := fmt.Sprintf("access_token:%d", userID)
	return client.Set(context.Background(), key, token, ttl).Err()
}

func GetAccessToken(userID int64) (string, error) {
	client := NewClient()
	defer client.Close()
	key := fmt.Sprintf("access_token:%d", userID)
	return client.Get(context.Background(), key).Result()
}
