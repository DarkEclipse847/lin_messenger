package redis

import "testing"

func TestInvalidRedisConfigurationDoesNotFallBack(t *testing.T) {
	t.Setenv("REDIS_URL", "invalid://secret@host")
	client, err := NewClient()
	if err == nil || client != nil {
		t.Fatal("invalid URL silently accepted")
	}
	if err.Error() != "invalid REDIS_URL" {
		t.Fatal("configuration error may expose credentials")
	}
}
