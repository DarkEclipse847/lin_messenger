package controllers

import (
	"encoding/json"

	"github.com/lin_auth/models"
	amqp "github.com/rabbitmq/amqp091-go"
	"github.com/rs/zerolog/log"
)

func rabbitSend(queueName string, user *models.User) {

	//Init connection to RabbitMQ
	conn, err := amqp.Dial("amqp://localhost:5672")
	if err != nil {
		log.Error().Err(err).Msg("Failed to connect to RabbitMQ")
	}
	defer conn.Close()
	
	//Open a channel
	channel, err := conn.Channel()
	if err != nil {
		log.Error().Err(err).Msg("Failed to connect open a channel")
	}
	defer channel.Close()

	// Declaring a queue
	//---- name --> durable --> autoDelete --> exclusive --> noWait --> error
	_, err = channel.QueueDeclare(queueName, true, false, false, false, nil)
	if err != nil {
		log.Error().Err(err).Msg("Failed to declare a queue")
	}
	userToJSON, err := json.Marshal(user)
	if err != nil {
		log.Error().Err(err).Msg("Failed to create JSON")
	}
	msg := amqp.Publishing{
		ContentType: "application/json",
		Body: userToJSON,
	}
	channel.Publish("", queueName, false, false, msg)	
}

//Recieving msg from rabbit
//https://stackoverflow.com/questions/53253797/make-go-routine-wait-until-result-from-rabbitmq-is-sent

//TODO: set a timeout, because rust server can be offline(no, but yeah)
func rabbitReceive(queueName string) <-chan string {
    conn, err := amqp.Dial("amqp://localhost:5672")
	if err != nil {
		log.Error().Err(err).Msg("Failed to connect to RabbitMQ")
	}
    ch, err := conn.Channel()
	if err != nil {
		log.Error().Err(err).Msg("Failed to create a channel")
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
	}

    msgs, err := ch.Consume(
        q.Name, // queue
        "consumer_go",     // consumer
        true,   // auto-ack
        false,  // exclusive
        false,  // no-local
        false,  // no-wait
        nil,    // args
    )
	if err != nil {
		log.Error().Err(err).Msg("Failed to consume msg")
	}

    resCh := make(chan string)
    go func() {
        d := <-msgs
        log.Printf("Received a message: %v from %v", string(d.Body), q.Name)
        resCh <- string(d.Body)
        conn.Close()
        ch.Close()
        close(resCh)
    }()
    return resCh
}