package controllers

import (
	"encoding/json"
	"errors"
	goredis "github.com/redis/go-redis/v9"
	"io"
	"math"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/models"
	"github.com/lin_auth/utils"
	amqp "github.com/rabbitmq/amqp091-go"
)

func TestRegistrationWireContract(t *testing.T) {
	fixture, err := os.ReadFile("../../contracts/registration_insert.json")
	if err != nil {
		t.Fatal(err)
	}
	user := models.User{Id: math.MaxInt64, Interaction_Type: "insert", Username: "alice", Email: "alice@example.test", Password: "must-not-be-published"}
	msg, err := registrationMessage(&user)
	if err != nil {
		t.Fatal(err)
	}
	// Decode into RawMessage to preserve all 64 bits of the JSON integer.
	var want, got map[string]json.RawMessage
	if err := json.Unmarshal(fixture, &want); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(msg.Body, &got); err != nil {
		t.Fatal(err)
	}
	if len(got) != len(want) {
		t.Fatal("unexpected event fields, possible credential exposure")
	}
	for key, value := range want {
		if string(got[key]) != string(value) {
			t.Fatalf("contract mismatch for %s", key)
		}
	}
	if msg.DeliveryMode != amqp.Persistent || msg.ContentType != "application/json" {
		t.Fatal("event must be persistent JSON")
	}
	user.Id = 0
	if _, err := registrationMessage(&user); err == nil {
		t.Fatal("invalid identity accepted")
	}
}

func TestTokenHeaderPrecedence(t *testing.T) {
	for _, tc := range []struct{ header, want string }{
		{"", "cookie-token"}, {"Bearer header-token", "header-token"}, {"bEaReR header-token", "header-token"},
		{"Basic invalid", ""}, {"Bearer", ""}, {"Bearer a b", ""},
	} {
		app := fiber.New()
		app.Get("/", func(c fiber.Ctx) error { return c.SendString(extractAccessToken(c)) })
		req := httptest.NewRequest("GET", "/", nil)
		req.Header.Set("Cookie", "access_token=cookie-token")
		req.Header.Set("Authorization", tc.header)
		res, err := app.Test(req)
		if err != nil {
			t.Fatal(err)
		}
		body, err := io.ReadAll(res.Body)
		res.Body.Close()
		if err != nil {
			t.Fatal(err)
		}
		if string(body) != tc.want {
			t.Fatalf("header %q: got %q", tc.header, body)
		}
	}
}

func TestBadCredentialsRejectedBeforeDatabaseAccess(t *testing.T) {
	for _, tc := range []struct{ route, body string }{
		{"/register", `{}`}, {"/register", `{"username":"//evil","email":"a@example.test","password":"password"}`},
		{"/register", `{"username":"alice","email":"not-mail","password":"password"}`},
		{"/register", `{"username":"alice","email":"a@example.test","password":"short"}`},
		{"/login", `{"password":"password"}`},
		{"/login", `{"username":"alice","email":"a@example.test","password":"password"}`},
		{"/login", `{"username":"alice","password":123}`},
		{"/login", `{"username":"alice","password":"` + strings.Repeat("x", 1025) + `"}`},
	} {
		app := fiber.New()
		app.Post("/register", func(c fiber.Ctx) error { return Register(c, nil) })
		app.Post("/login", func(c fiber.Ctx) error { return Login(c, nil) })
		req := httptest.NewRequest("POST", tc.route, strings.NewReader(tc.body))
		req.Header.Set("Content-Type", "application/json")
		res, err := app.Test(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != 400 {
			t.Fatalf("%s: expected 400, got %d", tc.body, res.StatusCode)
		}
	}
}

func TestCookiesHaveSecurityFlagsAndMatchingLifetimes(t *testing.T) {
	app := fiber.New()
	app.Get("/", func(c fiber.Ctx) error { setSessionCookies(c, "access", "refresh"); return c.SendStatus(200) })
	res, err := app.Test(httptest.NewRequest("GET", "/", nil))
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if len(res.Cookies()) != 2 {
		t.Fatal("expected both session cookies")
	}
	for _, cookie := range res.Cookies() {
		if !cookie.HttpOnly || !cookie.Secure || cookie.Path != "/" || cookie.SameSite == 0 {
			t.Fatal("missing cookie protections")
		}
		ttl := utils.AccessTokenTTL
		if cookie.Name == "refresh_token" {
			ttl = utils.RefreshTokenTTL
		}
		if delta := time.Until(cookie.Expires) - ttl; delta > time.Second || delta < -5*time.Second {
			t.Fatal("incorrect cookie lifetime")
		}
	}
}

func TestForwardedIdentityIsExact(t *testing.T) {
	t.Setenv("JWT_SECRET", "test-only-secret-with-at-least-32-bytes")
	token, err := utils.GenerateToken(math.MaxInt64, time.Now().Add(time.Minute).Unix())
	if err != nil {
		t.Fatal(err)
	}
	id, err := parseUserIDFromToken(token)
	if err != nil || id != math.MaxInt64 {
		t.Fatalf("identity corrupted: %d, %v", id, err)
	}
}

func TestGatewayValidationChecksStoredSessionAndForwardsExactID(t *testing.T) {
	t.Setenv("JWT_SECRET", "test-only-secret-with-at-least-32-bytes")
	token, err := utils.GenerateToken(math.MaxInt64, time.Now().Add(time.Minute).Unix())
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name, stored string
		lookupErr    error
		status       int
	}{
		{"active", token, nil, 200}, {"replaced", "another-token", nil, 401},
		{"expired", "", goredis.Nil, 401}, {"unavailable", "", errors.New("offline"), 503},
	} {
		t.Run(tc.name, func(t *testing.T) {
			app := fiber.New()
			app.Get("/api/auth/validate", func(c fiber.Ctx) error {
				return validateSession(c, func(id int64) (string, error) {
					if id != math.MaxInt64 {
						t.Errorf("incorrect Redis lookup ID: %d", id)
					}
					return tc.stored, tc.lookupErr
				})
			})
			req := httptest.NewRequest("GET", "/api/auth/validate", nil)
			req.Header.Set("Authorization", "Bearer "+token)
			res, err := app.Test(req)
			if err != nil {
				t.Fatal(err)
			}
			defer res.Body.Close()
			if res.StatusCode != tc.status {
				t.Fatalf("got status %d", res.StatusCode)
			}
			wantID := ""
			if tc.status == 200 {
				wantID = "9223372036854775807"
			}
			if res.Header.Get("X-User-Id") != wantID {
				t.Fatal("incorrect identity header")
			}
			if res.Header.Get("Cache-Control") != "no-store" {
				t.Fatal("validation response is cacheable")
			}
		})
	}
}
