// @generated automatically by Diesel CLI.

diesel::table! {
    nodes (public_key) {
        public_key -> Bytea,
        alias -> Text,
        capacity -> Numeric,
        first_seen -> Timestamptz,
        synced_at -> Timestamptz,
    }
}
