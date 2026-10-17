package redis

import (
	"context"
	"errors"
	"fmt"
	"os"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
)

const operationTimeout = 3 * time.Second

func NewClient() (*redis.Client, error) {
	redisURL := os.Getenv("REDIS_URL")
	if redisURL == "" {
		redisURL = "redis://localhost:6379/0"
	}
	opts, err := redis.ParseURL(redisURL)
	if err != nil {
		return nil, errors.New("invalid REDIS_URL")
	}
	opts.DialTimeout = operationTimeout
	opts.ReadTimeout = operationTimeout
	opts.WriteTimeout = operationTimeout
	opts.ContextTimeoutEnabled = true
	return redis.NewClient(opts), nil
}

// One pool for the process, not one connection pool per request. Configuration
// errors are preserved instead of silently connecting to a different Redis host.
var sharedClient = sync.OnceValues(NewClient)

func Initialize() error {
	_, err := sharedClient()
	return err
}

func StoreAccessToken(userID uuid.UUID, token string, ttl time.Duration) error {
	client, err := sharedClient()
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(context.Background(), operationTimeout)
	defer cancel()
	return client.Set(ctx, fmt.Sprintf("access_token:%s", userID), token, ttl).Err()
}

func GetAccessToken(userID uuid.UUID) (string, error) {
	client, err := sharedClient()
	if err != nil {
		return "", err
	}
	ctx, cancel := context.WithTimeout(context.Background(), operationTimeout)
	defer cancel()
	return client.Get(ctx, fmt.Sprintf("access_token:%s", userID)).Result()
}
