use uuid::Uuid;
//https://stackoverflow.com/questions/8351526/storing-chat-messages-inside-a-mysql-table?rq=4
//ID IS NOT SHARED

use chrono::NaiveDateTime;
use diesel::prelude::*;

// Do i really need to store chat ids in user?
#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = crate::database::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: Uuid,
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
    // The database permits chats without an administrator.
    pub admin: Option<Uuid>,
    pub chat_img: Option<String>,
    pub bio: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Queryable, Selectable, Insertable, Debug)]
#[diesel(table_name = crate::database::schema::messages)]
pub struct Message {
    pub id: i64,
    pub chat_id: Option<i64>,
    pub sender_id: Option<Uuid>,
    pub body: Option<String>,
    pub sent_at: Option<NaiveDateTime>,
    pub attachment: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::messages)]

pub struct NewMessage<'a> {
    pub id: &'a i64,
    pub chat_id: &'a i64,
    pub sender_id: &'a Uuid,
    pub body: &'a str,
    pub sent_at: &'a NaiveDateTime,
    pub attachment: Option<&'a str>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::chats)]
pub struct NewChat<'a> {
    pub id: &'a i64,
    pub title: &'a String,
    pub admin: &'a Uuid,
    pub created_at: &'a NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = crate::database::schema::users)]
pub struct NewUser<'a> {
    pub id: &'a Uuid,
    pub username: &'a str,
    pub email: &'a str,
    pub registred_at: &'a NaiveDateTime,
}
