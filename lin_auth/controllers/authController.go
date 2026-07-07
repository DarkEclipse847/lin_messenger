// TODO: Parse google creds, create google oauth controller
// https://www.youtube.com/watch?v=tgO_ADSvY1I

package controllers

import (
	"time"

	"github.com/gofiber/fiber/v3"
	"github.com/lin_auth/database"
	"github.com/lin_auth/utils"
	"github.com/rs/zerolog/log"
	"gorm.io/gorm"
)

func Register(c fiber.Ctx, db *gorm.DB) error {
	var data map[string]string

	password := utils.GenerateHashFromPasswd(data["password"])

	

	user, err := database.CreateUser(db, data["username"], data["email"], password)
	if err != nil{
		log.Error().Msg(err.Error())
		return err
	}

	rabbitSend("REGISTRATION_REQ", &user)

	access_token, refresh_token, err := utils.GenerateTokenPair(user.Id)
	if err != nil {
		log.Error().Msg(err.Error())
		return err
	}

	c.Cookie(&fiber.Cookie{
		Name: "access_token",
		Value: access_token,
		Expires: time.Now().Add(15*time.Minute),
		HTTPOnly: true, // Like readonly but for cookies
		Secure: true, //HTTPS only
		Path: "/", //Website-wide cookie usage
	})
	c.Cookie(&fiber.Cookie{
		Name: "refresh_token",
		Value: refresh_token,
		HTTPOnly: true, // Like readonly but for cookies
		Secure: true, //HTTPS only
		Path: "/", //Website-wide cookie usage
	})

	
	

	//response := <-rabbitReceive("REGISTRATION_RES")

	//resParsed, err := strconv.ParseInt(response, 10, 64)
	//if err != nil {
	//	log.Error().Err(err).Msgf("Wrong type of response: %s", response)
	//}
	//TODO: add error case to the response
	//user.Id = resParsed


	//Parse the data into a struct
	//if err := c.Bind().Body(&data); err != nil {
	//	return err
	//}

    //REDIRECT USER TO lin.wtf/{id}
	return c.Redirect().Status(fiber.StatusMovedPermanently).To("/"+user.Username)
}

func Login(c fiber.Ctx, db *gorm.DB)  {
}
