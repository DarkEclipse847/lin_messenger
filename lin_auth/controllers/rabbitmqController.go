package controllers

import (
	"encoding/json"
	"os"

	"github.com/lin_auth/models"
	amqp "github.com/rabbitmq/amqp091-go"
	"github.com/rs/zerolog/log"
)

// rabbitmqURL resolves the broker address from RABBITMQ_URL, matching the
// connection string configured in compose.yaml, falling back to a local
// broker for standalone (non-docker) runs.
func rabbitmqURL() string {
	url := os.Getenv("RABBITMQ_URL")
	if url == "" {
		log.Error().Msg("RabbitMQ URL not found, using loopback to localhost")
		url = "amqp://localhost:5672"
	}
	return url
}

func rabbitSend(queueName string, user *models.User) {

	//Init connection to RabbitMQ
	conn, err := amqp.Dial(rabbitmqURL())
	if err != nil {
		log.Error().Err(err).Msg("Failed to connect to RabbitMQ")
		return
	}
	defer conn.Close()

	//Open a channel
	channel, err := conn.Channel()
	if err != nil {
		log.Error().Err(err).Msg("Failed to open a channel")
		return
	}
	defer channel.Close()

	// Declaring a queue
	//---- name --> durable --> autoDelete --> exclusive --> noWait --> error
	_, err = channel.QueueDeclare(queueName, true, false, false, false, nil)
	if err != nil {
		log.Error().Err(err).Msg("Failed to declare a queue")
		return
	}

	userToJSON, err := json.Marshal(user)
	if err != nil {
		log.Error().Err(err).Msg("Failed to create JSON")
		return
	}

	msg := amqp.Publishing{
		ContentType: "application/json",
		Body:        userToJSON,
	}

	if err := channel.Publish("", queueName, false, false, msg); err != nil {
		log.Error().Err(err).Msg("Failed to publish message")
	}
}

//Recieving msg from rabbit
//https://stackoverflow.com/questions/53253797/make-go-routine-wait-until-result-from-rabbitmq-is-sent

//TODO: set a timeout, because rust server can be offline(no, but yeah)
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
