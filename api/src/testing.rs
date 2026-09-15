//! Fixtures shared by the unit tests, so a node used in an assertion reads the
//! same everywhere.

use time::macros::datetime;

use crate::domain::Node;

/// A node with a known first-seen date, so assertions can pin the JSON.
pub fn node(public_key: &str, alias: &str, satoshis: i64) -> Node {
    Node {
        public_key: public_key.parse().expect("a valid public key"),
        alias: alias.to_string(),
        capacity: Node::capacity_from_satoshis(satoshis),
        first_seen: datetime!(2018-04-05 15:13:42 UTC),
    }
}
