// @generated automatically by Diesel CLI.

diesel::table! {
    chats (id) {
        id -> Int8,
        title -> Text,
        chat_img -> Nullable<Text>,
        admin -> Nullable<Int8>,
        bio -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    messages (id) {
        id -> Int8,
        chat_id -> Int8,
        sender_id -> Int8,
        body -> Text,
        sent_at -> Timestamp,
        attachment -> Nullable<Text>,
    }
}

diesel::table! {
    users (id) {
        id -> Int8,
        username -> Text,
        email -> Text,
        phone -> Nullable<Text>,
        displayed_name -> Nullable<Text>,
        profile_img -> Nullable<Text>,
        bio -> Nullable<Text>,
        registred_at -> Timestamp,
    }
}

diesel::joinable!(chats -> users (admin));
diesel::joinable!(messages -> chats (chat_id));
diesel::joinable!(messages -> users (sender_id));

diesel::allow_tables_to_appear_in_same_query!(chats, messages, users,);
