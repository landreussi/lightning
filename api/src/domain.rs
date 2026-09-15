use rust_decimal::Decimal;
pub use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// How many decimal places a satoshi amount has once expressed in BTC.
const SATOSHI_SCALE: u32 = 8;

/// A lightning node as this service stores and serves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub public_key: PublicKey,

    pub alias: String,

    /// Total channel capacity, in BTC. A string in JSON: the value is exact to
    /// the satoshi and a float would not keep it that way.
    #[serde(with = "rust_decimal::serde::str")]
    pub capacity: Decimal,

    /// When the node was first seen on the network.
    #[serde(with = "time::serde::rfc3339")]
    pub first_seen: OffsetDateTime,
}

impl Node {
    /// Converts a satoshi amount to the BTC figure the node is stored with.
    /// Exact: it only moves the decimal point, it doesn't divide.
    pub fn capacity_from_satoshis(satoshis: i64) -> Decimal {
        Decimal::new(satoshis, SATOSHI_SCALE)
    }
}

#[cfg(test)]
impl Node {
    pub const ACINQ: &str = "03864ef025fde8fb587d989186ce6a4a186895ee44a926bfc370e2c366597a3f8f";
    pub const WOS: &str = "035e4ff418fc8b5554c5d9eea66396c227bd429a3251c8cbc711002ba215bfc226";
}

impl TryFrom<mempool::Node> for Node {
    type Error = secp256k1::Error;

    fn try_from(ranking: mempool::Node) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: ranking.public_key.parse()?,
            alias: ranking.alias,
            capacity: Self::capacity_from_satoshis(ranking.capacity),
            first_seen: ranking.first_seen,
        })
    }
}

#[cfg(test)]
mod tests {
    use secp256k1::constants::PUBLIC_KEY_SIZE;
    use time::macros::datetime;

    use super::*;

    #[test]
    fn a_public_key_round_trips_through_hex() {
        let key: PublicKey = Node::ACINQ.parse().unwrap();

        assert_eq!(key.serialize().len(), PUBLIC_KEY_SIZE);
        assert_eq!(key.to_string(), Node::ACINQ);
        assert_eq!(PublicKey::from_slice(&key.serialize()).unwrap(), key);
    }

    #[test]
    fn a_public_key_of_the_wrong_length_is_rejected() {
        assert!("03864ef0".parse::<PublicKey>().is_err());
    }

    #[test]
    fn a_public_key_that_is_not_hex_is_rejected() {
        let not_hex = "z".repeat(PUBLIC_KEY_SIZE * 2);

        assert!(not_hex.parse::<PublicKey>().is_err());
    }

    /// Hex of the right length still has to be a point on the curve.
    #[test]
    fn a_public_key_that_is_not_on_the_curve_is_rejected() {
        let off_curve = format!("02{}", "ff".repeat(PUBLIC_KEY_SIZE - 1));

        assert!(off_curve.parse::<PublicKey>().is_err());
    }

    /// The shape the API promises: hex key, BTC capacity as a string, RFC 3339
    /// timestamp.
    #[test]
    fn a_node_serialises_the_way_the_api_documents_it() {
        let node = Node::try_from(mempool::Node {
            public_key: Node::ACINQ.to_string(),
            alias: "ACINQ".to_string(),
            capacity: 36_010_516_297,
            first_seen: datetime!(2018-04-05 15:13:42 UTC),
        })
        .unwrap();

        assert_eq!(
            serde_json::to_value(&node).unwrap(),
            serde_json::json!({
                "public_key": Node::ACINQ,
                "alias": "ACINQ",
                "capacity": "360.10516297",
                "first_seen": "2018-04-05T15:13:42Z",
            })
        );
    }

    #[test]
    fn satoshis_become_btc_without_rounding() {
        assert_eq!(
            Node::capacity_from_satoshis(1).to_string(),
            "0.00000001".to_string()
        );
        assert_eq!(
            Node::capacity_from_satoshis(2_100_000_000_000_000).to_string(),
            "21000000.00000000".to_string()
        );
    }
}
