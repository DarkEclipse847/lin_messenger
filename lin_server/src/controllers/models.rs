use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct UserRabbit {
    pub interaction_type: String,
    pub id: Uuid,
    pub username: String,
    pub email: String,
}
