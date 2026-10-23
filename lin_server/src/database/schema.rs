// @generated automatically by Diesel CLI.

diesel::table! {
    chat_members (chat_id, user_id) {
        chat_id -> Int8,
        user_id -> Uuid,
    }
}

diesel::table! {
    chats (id) {
        id -> Int8,
        title -> Text,
        chat_img -> Nullable<Text>,
        admin -> Nullable<Uuid>,
        bio -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    messages (id) {
        id -> Int8,
        chat_id -> Nullable<Int8>,
        sender_id -> Nullable<Uuid>,
        body -> Nullable<Text>,
        sent_at -> Nullable<Timestamp>,
        attachment -> Nullable<Text>,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        username -> Text,
        email -> Text,
        phone -> Nullable<Text>,
        displayed_name -> Nullable<Text>,
        profile_img -> Nullable<Text>,
        bio -> Nullable<Text>,
        registred_at -> Timestamp,
    }
}

diesel::joinable!(chat_members -> chats (chat_id));
diesel::joinable!(chat_members -> users (user_id));
diesel::joinable!(chats -> users (admin));
diesel::joinable!(messages -> chats (chat_id));
diesel::joinable!(messages -> users (sender_id));

diesel::allow_tables_to_appear_in_same_query!(chat_members, chats, messages, users,);
