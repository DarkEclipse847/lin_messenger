package controllers

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"os"
	"time"

	"github.com/google/uuid"
	"github.com/lin_auth/models"
	amqp "github.com/rabbitmq/amqp091-go"
	"github.com/rs/zerolog/log"
)

func rabbitmqURL() string {
	if url := os.Getenv("RABBITMQ_URL"); url != "" {
		return url
	}
	return "amqp://guest:guest@localhost:5672/%2F"
}

func registrationMessage(user *models.User) (amqp.Publishing, error) {
	if user.Id == uuid.Nil || user.Interaction_Type != "insert" {
		return amqp.Publishing{}, errors.New("invalid registration event")
	}
	body, err := json.Marshal(user)
	return amqp.Publishing{ContentType: "application/json", DeliveryMode: amqp.Persistent, Body: body}, err
}

func rabbitSend(queueName string, user *models.User) error {
	msg, err := registrationMessage(user)
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	deadline, _ := ctx.Deadline()
	conn, err := amqp.DialConfig(rabbitmqURL(), amqp.Config{
		Dial: func(network, address string) (net.Conn, error) {
			socket, err := (&net.Dialer{}).DialContext(ctx, network, address)
			if err != nil {
				return nil, err
			}
			if err := socket.SetDeadline(deadline); err != nil {
				socket.Close()
				return nil, err
			}
			// The AMQP library clears handshake deadlines, so cancellation
			// must also close the socket to bound later channel RPCs/writes.
			context.AfterFunc(ctx, func() { _ = socket.Close() })
			return socket, nil
		},
	})
	if err != nil {
		return err
	}
	defer conn.Close()
	channel, err := conn.Channel()
	if err != nil {
		return err
	}
	defer channel.Close()
	if _, err := channel.QueueDeclare(queueName, true, false, false, false, nil); err != nil {
		return err
	}
	if err := channel.Confirm(false); err != nil {
		return err
	}
	returned := channel.NotifyReturn(make(chan amqp.Return, 1))
	confirmation, err := channel.PublishWithDeferredConfirm("", queueName, true, false, msg)
	if err != nil {
		return err
	}
	confirmed, err := confirmation.WaitContext(ctx)
	if err != nil {
		return err
	}
	if !confirmed {
		return errors.New("registration publication rejected")
	}
	select {
	case <-returned:
		return errors.New("registration event was not routed")
	default:
		return nil
	}
}

//Recieving msg from rabbit
//https://stackoverflow.com/questions/53253797/make-go-routine-wait-until-result-from-rabbitmq-is-sent

// TODO: set a timeout, because rust server can be offline(no, but yeah)
func rabbitReceive(queueName string) <-chan string {
	resCh := make(chan string)

	conn, err := amqp.Dial(rabbitmqURL())
	if err != nil {
		log.Error().Err(err).Msg("Failed to connect to RabbitMQ")
		close(resCh)
		return resCh
	}

	ch, err := conn.Channel()
	if err != nil {
		log.Error().Err(err).Msg("Failed to create a channel")
		conn.Close()
		close(resCh)
		return resCh
	}

	q, err := ch.QueueDeclare(
		queueName, // name
		true,      // durable
		false,     // delete when usused
		false,     // exclusive
		false,     // no-waits
		nil,       // arguments
	)
	if err != nil {
		log.Error().Err(err).Msg("Failed to declare a queue")
		ch.Close()
		conn.Close()
		close(resCh)
		return resCh
	}

	msgs, err := ch.Consume(
		q.Name,        // queue
		"consumer_go", // consumer
		true,          // auto-ack
		false,         // exclusive
		false,         // no-local
		false,         // no-wait
		nil,           // args
	)
	if err != nil {
		log.Error().Err(err).Msg("Failed to consume msg")
		ch.Close()
		conn.Close()
		close(resCh)
		return resCh
	}

	go func() {
		d := <-msgs
		log.Info().Msgf("Received a message: %v from %v", string(d.Body), q.Name)
		resCh <- string(d.Body)
		conn.Close()
		ch.Close()
		close(resCh)
	}()

	return resCh
}
