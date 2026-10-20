package utils

import (
	"strings"
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
)

const testKey = "test-only-secret-with-at-least-32-bytes"

func TestTokensPreserveIdentityAndSeparatePurposes(t *testing.T) {
	t.Setenv("JWT_SECRET", testKey)
	access, refresh, err := GenerateTokenPair(uuid.MustParse("550e8400-e29b-41d4-a716-446655440000"))
	if err != nil {
		t.Fatal(err)
	}
	parsed, err := ValidateToken(access)
	if err != nil {
		t.Fatal(err)
	}
	if parsed.Claims.(*TokenClaims).UserID != uuid.MustParse("550e8400-e29b-41d4-a716-446655440000") {
		t.Fatal("user UUID changed")
	}
	if _, err := ValidateToken(refresh); err == nil {
		t.Fatal("refresh token accepted as access token")
	}
	// Same-second session replacement must produce distinct credentials.
	seen := map[string]bool{access: true, refresh: true}
	for range 10 {
		a, r, err := GenerateTokenPair(uuid.MustParse("550e8400-e29b-41d4-a716-446655440000"))
		if err != nil {
			t.Fatal(err)
		}
		for _, token := range []string{a, r} {
			if seen[token] {
				t.Fatal("repeated session credential")
			}
			seen[token] = true
		}
	}
}

func TestRejectInvalidTokenClaimsAndSigningMethods(t *testing.T) {
	t.Setenv("JWT_SECRET", testKey)
	for _, tc := range []struct {
		name  string
		field string
		value any
	}{
		{"missing expiry", "exp", nil}, {"expired", "exp", time.Now().Add(-time.Minute).Unix()},
		{"nil UUID", "user_id", uuid.Nil.String()}, {"missing identity", "user_id", nil}, {"malformed UUID", "user_id", "not-a-uuid"}, {"legacy identity", "user_id", 42}, {"zero identity", "user_id", 0}, {"negative identity", "user_id", -1},
		{"fractional identity", "user_id", 1.5}, {"overflow", "user_id", uint64(18446744073709551615)},
		{"missing purpose", "token_type", nil}, {"refresh purpose", "token_type", "refresh"},
		{"missing nonce", "jti", nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			claims := jwt.MapClaims{"user_id": "550e8400-e29b-41d4-a716-446655440000", "exp": time.Now().Add(time.Hour).Unix(), "token_type": "access", "jti": "test"}
			if tc.value == nil {
				delete(claims, tc.field)
			} else {
				claims[tc.field] = tc.value
			}
			token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(testKey))
			if err != nil {
				t.Fatal(err)
			}
			if _, err := ValidateToken(token); err == nil {
				t.Fatal("invalid token accepted")
			}
		})
	}
	claims := jwt.MapClaims{"user_id": "550e8400-e29b-41d4-a716-446655440000", "exp": time.Now().Add(time.Hour).Unix(), "token_type": "access", "jti": "test"}
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS512, claims).SignedString([]byte(testKey))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ValidateToken(token); err == nil {
		t.Fatal("HS512 token accepted")
	}
}

func TestSigningKeyFailsClosed(t *testing.T) {
	for _, secret := range []string{"", "short", strings.Repeat("a", 31)} {
		t.Setenv("JWT_SECRET", secret)
		if _, err := GenerateToken(uuid.MustParse("550e8400-e29b-41d4-a716-446655440000"), time.Now().Add(time.Hour).Unix()); err == nil {
			t.Fatal("weak key accepted")
		}
		if _, err := ValidateToken("anything"); err == nil {
			t.Fatal("validation accepted weak key")
		}
	}
}
