use serde::Deserialize;


#[derive(Deserialize)]
pub struct UserRabbit {
    pub interaction_type: String,
    pub id: i64,
    pub username: String,
    pub email: String,
}