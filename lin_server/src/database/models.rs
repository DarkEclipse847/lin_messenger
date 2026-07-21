//https://stackoverflow.com/questions/8351526/storing-chat-messages-inside-a-mysql-table?rq=4
//ID IS NOT SHARED

use diesel::prelude::*;
use chrono::NaiveDateTime;


// Do i really need to store chat ids in user?
#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = crate::database::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub phone: Option<String>,
    pub displayed_name: Option<String>,
    // link to S3
    pub profile_img: Option<String>,
    pub bio: Option<String>,
    pub registred_at: NaiveDateTime,

}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::database::schema::chats)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Chat {
    pub id: i64,
    pub title: String,
    // I use Option because that it's joinable, so generated schema do not let me set strict
    // i64, i'll need to change that later, or stick with it
    pub admin: Option<i64>,
    pub chat_img: Option<String>,
    pub bio: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}


#[derive(Queryable, Selectable, Insertable, Debug)]
#[diesel(table_name = crate::database::schema::messages)]
pub struct Message {
    pub id: i64,
    pub chat_id: i64,
    pub sender_id: i64,
    pub body: String,
    pub sent_at: NaiveDateTime,
    pub attachment: Option<String>
}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::messages)]

pub struct NewMessage<'a> {
    pub chat_id: &'a i64,
    pub sender_id: &'a i64,
    pub body: &'a str,
    pub sent_at: &'a NaiveDateTime,
    pub attachment: Option<&'a str>,

}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::chats)]
pub struct NewChat<'a> {
    pub id: &'a i64,
    pub title: &'a String,
    pub admin: &'a i64,
    pub created_at: &'a NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::users)]
pub struct NewUser<'a> {
    pub id: &'a i64,
    pub username: &'a str,
    pub email: &'a str,
    pub registred_at: &'a NaiveDateTime,
}