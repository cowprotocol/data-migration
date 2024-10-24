use std::fmt::{self, Display};

use {
    crate::serialization::HexOrDecimalU256,
    derivative::Derivative,
    primitive_types::{H160, H256, U256},
    serde::{de, Deserializer, Serializer},
    serde::{Deserialize, Serialize},
    serde_with::serde_as,
    std::collections::BTreeMap,
};

// uid as 56 bytes: 32 for orderDigest, 20 for ownerAddress and 4 for validTo
#[derive(Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct OrderUid(pub [u8; 56]);

impl Display for OrderUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = [0u8; 2 + 56 * 2];
        bytes[..2].copy_from_slice(b"0x");
        // Unwrap because the length is always correct.
        hex::encode_to_slice(self.0.as_slice(), &mut bytes[2..]).unwrap();
        // Unwrap because the string is always valid utf8.
        let str = std::str::from_utf8(&bytes).unwrap();
        f.write_str(str)
    }
}

impl fmt::Debug for OrderUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for OrderUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> Deserialize<'de> for OrderUid {
    fn deserialize<D>(deserializer: D) -> Result<OrderUid, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor {}
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = OrderUid;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "an uid with orderDigest_owner_validTo")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let s = s.strip_prefix("0x").ok_or_else(|| {
                    de::Error::custom(format!(
                        "{s:?} can't be decoded as hex uid because it does not start with '0x'"
                    ))
                })?;
                let mut value = [0u8; 56];
                hex::decode_to_slice(s, value.as_mut()).map_err(|err| {
                    de::Error::custom(format!("failed to decode {s:?} as hex uid: {err}"))
                })?;
                Ok(OrderUid(value))
            }
        }

        deserializer.deserialize_str(Visitor {})
    }
}

/// Stored directly in the database and turned into SolverCompetitionAPI for the
/// `/solver_competition` endpoint.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SolverCompetitionDB {
    pub auction_start_block: u64,
    pub competition_simulation_block: u64,
    pub auction: CompetitionAuction,
    pub solutions: Vec<SolverSettlement>,
}

/// Returned by the `/solver_competition` endpoint.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SolverCompetitionAPI {
    #[serde(default)]
    pub auction_id: i64,
    pub transaction_hashes: Vec<H256>,
    #[serde(flatten)]
    pub common: SolverCompetitionDB,
}

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionAuction {
    pub orders: Vec<OrderUid>,
    #[serde_as(as = "BTreeMap<_, HexOrDecimalU256>")]
    pub prices: BTreeMap<H160, U256>,
}

#[serde_as]
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Derivative)]
#[derivative(Debug)]
#[serde(rename_all = "camelCase")]
pub struct SolverSettlement {
    pub solver: String,
    #[serde(default)]
    pub solver_address: H160,
    #[serde(flatten)]
    pub score: Option<Score>,
    #[serde(default)]
    pub ranking: usize,
    #[serde_as(as = "BTreeMap<_, HexOrDecimalU256>")]
    pub clearing_prices: BTreeMap<H160, U256>,
    pub orders: Vec<Order>,
}

#[serde_as]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Score {
    /// The score is provided by the solver.
    #[serde(rename = "score")]
    Solver(#[serde_as(as = "HexOrDecimalU256")] U256),
    /// The score is calculated by the protocol (and equal to the objective
    /// function).
    #[serde(rename = "scoreProtocol")]
    Protocol(#[serde_as(as = "HexOrDecimalU256")] U256),
    /// The score is calculated by the protocol and success_probability provided
    /// by solver is taken into account
    #[serde(rename = "scoreProtocolWithSolverRisk")]
    ProtocolWithSolverRisk(#[serde_as(as = "HexOrDecimalU256")] U256),
    /// The score is calculated by the protocol, by applying a discount to the
    /// `Self::Protocol` value.
    /// [DEPRECATED] Kept to not brake the solver competition API.
    #[serde(rename = "scoreDiscounted")]
    Discounted(#[serde_as(as = "HexOrDecimalU256")] U256),
}

impl Default for Score {
    fn default() -> Self {
        Self::Protocol(Default::default())
    }
}

impl Score {
    pub fn score(&self) -> U256 {
        match self {
            Self::Solver(score) => *score,
            Self::Protocol(score) => *score,
            Self::ProtocolWithSolverRisk(score) => *score,
            Self::Discounted(score) => *score,
        }
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Order {
    #[serde(rename_all = "camelCase")]
    Colocated {
        id: OrderUid,
        /// The effective amount that left the user's wallet including all fees.
        #[serde_as(as = "HexOrDecimalU256")]
        sell_amount: U256,
        /// The effective amount the user received after all fees.
        #[serde_as(as = "HexOrDecimalU256")]
        buy_amount: U256,
    },
    #[serde(rename_all = "camelCase")]
    Legacy {
        id: OrderUid,
        #[serde_as(as = "HexOrDecimalU256")]
        executed_amount: U256,
    },
}
