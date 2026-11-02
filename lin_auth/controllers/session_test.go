package controllers

import (
	"errors"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gofiber/fiber/v3"
	"github.com/google/uuid"
	"github.com/lin_auth/database"
	"github.com/lin_auth/utils"
)

func TestRefreshAndLogout(t *testing.T) {
	t.Setenv("JWT_SECRET", "test-only-secret-with-at-least-32-bytes")
	id := uuid.New()
	access, refresh, err := utils.GenerateTokenPair(id)
	if err != nil {
		t.Fatal(err)
	}
	for _, logout := range []bool{false, true} {
		for _, tc := range []struct {
			name, token string
			failure     error
			status      int
		}{
			{"valid", refresh, nil, 204}, {"missing", "", nil, 401},
			{"malformed", "invalid", nil, 401}, {"wrong purpose", access, nil, 401},
			{"replayed", refresh, database.ErrInvalidSession, 401},
			{"store unavailable", refresh, errors.New("offline"), 503},
		} {
			t.Run(tc.name+map[bool]string{true: " logout", false: " refresh"}[logout], func(t *testing.T) {
				called := false
				app := fiber.New()
				app.Post("/", func(c fiber.Ctx) error {
					return changeRefreshSession(c, logout, func(got uuid.UUID, current, nextAccess, nextRefresh string) error {
						called = true
						if got != id || current != refresh {
							t.Error("incorrect session identity")
						}
						if logout {
							if nextAccess != "" || nextRefresh != "" {
								t.Error("logout issued tokens")
							}
						} else {
							if nextAccess == access || nextRefresh == refresh {
								t.Error("tokens not rotated")
							}
							if _, err := utils.ValidateToken(nextAccess); err != nil {
								t.Error(err)
							}
							if _, err := utils.ValidateRefreshToken(nextRefresh); err != nil {
								t.Error(err)
							}
						}
						return tc.failure
					})
				})
				req := httptest.NewRequest("POST", "/", nil)
				req.Header.Set("Cookie", "refresh_token="+tc.token)
				res, err := app.Test(req)
				if err != nil {
					t.Fatal(err)
				}
				defer res.Body.Close()
				if res.StatusCode != tc.status {
					t.Fatalf("status %d", res.StatusCode)
				}
				if called != (tc.token == refresh) {
					t.Fatal("invalid token reached storage")
				}
				if res.Header.Get("Cache-Control") != "no-store" {
					t.Fatal("cacheable response")
				}
				if tc.status != 204 {
					if len(res.Cookies()) != 0 {
						t.Fatal("failure changed cookies")
					}
					return
				}
				if len(res.Cookies()) != 2 {
					t.Fatal("expected both cookies")
				}
				for _, cookie := range res.Cookies() {
					if !cookie.Secure || !cookie.HttpOnly || cookie.Path != "/" || cookie.SameSite == 0 {
						t.Fatal("missing cookie protection")
					}
					if logout && (cookie.Value != "" || !cookie.Expires.Before(time.Now())) {
						t.Fatal("cookie not cleared")
					}
				}
			})
		}
	}
}
