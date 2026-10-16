package utils

import (
	"strings"
	"testing"
)

func TestPasswordHashingAndVerification(t *testing.T) {
	hash, err := GenerateHashFromPasswd("test-only-password")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(hash, "$argon2id$") {
		t.Fatal("expected Argon2id hash")
	}
	if !ComparePassword(hash, "test-only-password") {
		t.Fatal("correct password rejected")
	}
	if ComparePassword(hash, "wrong-password") {
		t.Fatal("wrong password accepted")
	}
	if ComparePassword("malformed-hash", "test-only-password") {
		t.Fatal("malformed hash accepted")
	}
}
