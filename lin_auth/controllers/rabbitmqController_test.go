package controllers

import (
	"bytes"
	"context"
	"errors"
	"github.com/google/uuid"
	"github.com/lin_auth/models"
	amqp "github.com/rabbitmq/amqp091-go"
	"io"
	"net"
	"net/url"
	"os"
	"sync/atomic"
	"testing"
	"time"
)

func TestRegistrationRetries(t *testing.T) {
	for _, failures := range []int{0, 1, 2, 3} {
		calls := 0
		var delays []time.Duration
		offline := errors.New("offline")
		err := retryRegistration(context.Background(), func(ctx context.Context) error {
			calls++
			if _, ok := ctx.Deadline(); !ok {
				t.Fatal("unbounded publish attempt")
			}
			if calls <= failures {
				return offline
			}
			return nil
		}, func(_ context.Context, delay time.Duration) error { delays = append(delays, delay); return nil })
		want := failures + 1
		if want > 3 {
			want = 3
		}
		if calls != want || (err != nil) != (failures == 3) {
			t.Fatalf("failures=%d calls=%d err=%v", failures, calls, err)
		}
		for i, delay := range delays {
			if delay != time.Second<<i {
				t.Fatalf("delay=%v", delay)
			}
		}
	}
}

func TestRegistrationRetryCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	calls := 0
	err := retryRegistration(ctx, func(context.Context) error {
		calls++
		cancel()
		return errors.New("offline")
	}, waitRegistrationRetry)
	if !errors.Is(err, context.Canceled) || calls != 1 {
		t.Fatalf("calls=%d err=%v", calls, err)
	}
	if err := waitRegistrationRetry(ctx, time.Hour); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

// Uses an isolated queue on a disposable broker. Drop two real AMQP connections
// and verify the third connection publishes the unchanged persistent event.
func TestRegistrationRetryReopensConnection(t *testing.T) {
	broker := os.Getenv("LIN_TEST_RABBITMQ_URL")
	if broker == "" {
		t.Skip("set LIN_TEST_RABBITMQ_URL to a disposable AMQP broker")
	}
	target, err := url.Parse(broker)
	if err != nil || target.Scheme != "amqp" {
		t.Fatal("test requires an amqp URL")
	}
	conn, err := amqp.Dial(broker)
	if err != nil {
		t.Fatal("test broker unavailable")
	}
	defer conn.Close()
	ch, err := conn.Channel()
	if err != nil {
		t.Fatal(err)
	}
	defer ch.Close()
	queue := "lin_registration_retry_" + uuid.NewString()
	defer ch.QueueDelete(queue, false, false, false)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	var attempts atomic.Int32
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			client, err := listener.Accept()
			if err != nil {
				return
			}
			if attempts.Add(1) < 3 {
				client.Close()
				continue
			}
			upstream, err := net.DialTimeout("tcp", target.Host, time.Second)
			if err != nil {
				client.Close()
				return
			}
			_ = client.SetDeadline(time.Now().Add(10 * time.Second))
			_ = upstream.SetDeadline(time.Now().Add(10 * time.Second))
			copied := make(chan struct{})
			go func() { _, _ = io.Copy(upstream, client); upstream.Close(); close(copied) }()
			_, _ = io.Copy(client, upstream)
			client.Close()
			<-copied
			return
		}
	}()
	proxy := *target
	proxy.Host = listener.Addr().String()
	t.Setenv("RABBITMQ_URL", proxy.String())
	user := models.User{Id: uuid.New(), Interaction_Type: "insert", Username: "retry", Email: "retry@example.test", Password: "must-not-publish"}
	if err := rabbitSend(queue, &user); err != nil {
		t.Fatal("publication did not recover")
	}
	<-done
	if attempts.Load() != 3 {
		t.Fatalf("attempts=%d", attempts.Load())
	}
	delivery, ok, err := ch.Get(queue, true)
	if err != nil || !ok {
		t.Fatal("confirmed event missing")
	}
	expected, _ := registrationMessage(&user)
	if !bytes.Equal(delivery.Body, expected.Body) || delivery.DeliveryMode != amqp.Persistent {
		t.Fatal("registration payload or persistence changed during retry")
	}
}
