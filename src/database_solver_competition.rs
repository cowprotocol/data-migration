use bigdecimal::BigDecimal;
use num::{BigInt, BigUint};
use primitive_types::U256;
use sqlx::{
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueFormat, PgValueRef},
    types::JsonValue,
    Decode, Encode, PgConnection, Postgres, Type,
};
use std::fmt::{self, Debug, Formatter};

/// Wrapper type for fixed size byte arrays compatible with sqlx's Postgres
/// implementation.
#[derive(Clone, Copy, Eq, PartialEq, Hash, sqlx::FromRow)]
pub struct ByteArray<const N: usize>(pub [u8; N]);

impl<const N: usize> Debug for ByteArray<N> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl<const N: usize> Default for ByteArray<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> Type<Postgres> for ByteArray<N> {
    fn type_info() -> PgTypeInfo {
        <[u8] as Type<Postgres>>::type_info()
    }
}

impl<const N: usize> PgHasArrayType for ByteArray<N> {
    fn array_type_info() -> PgTypeInfo {
        <[&[u8]] as Type<Postgres>>::type_info()
    }
}

impl<const N: usize> Decode<'_, Postgres> for ByteArray<N> {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let mut bytes = [0u8; N];
        match value.format() {
            // prepared query
            PgValueFormat::Binary => {
                bytes = value.as_bytes()?.try_into()?;
            }
            // unprepared raw query
            PgValueFormat::Text => {
                let text = value
                    .as_bytes()?
                    .strip_prefix(b"\\x")
                    .ok_or("text does not start with \\x")?;
                hex::decode_to_slice(text, &mut bytes)?
            }
        };
        Ok(Self(bytes))
    }
}

impl<const N: usize> Encode<'_, Postgres> for ByteArray<N> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> IsNull {
        self.0.encode(buf)
    }
}

pub type Address = ByteArray<20>;
pub type OrderUid = ByteArray<56>;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RichSolverCompetition {
    pub id: i64,
    pub json: JsonValue,
    pub deadline: i64,
    pub surplus_capturing_jit_order_owners: Vec<Address>,
}

/// Migrate all the auctions from the solver_competitions table to the auctions
/// table. This is a one-time migration.
///
/// Entries are fetched going from higher auction_id to lower auction_id.
pub async fn fetch_batch(
    ex: &mut PgConnection,
    auction_id: i64,
    batch_size: i64,
) -> Result<Vec<RichSolverCompetition>, sqlx::Error> {
    const QUERY: &str = r#"
        SELECT 
        sc.id as id, 
        sc.json as json, 
        COALESCE(ss.block_deadline, 0) AS deadline,
        COALESCE(jit.owners, ARRAY[]::bytea[]) AS surplus_capturing_jit_order_owners
        FROM solver_competitions sc
        LEFT JOIN settlement_scores ss ON sc.id = ss.auction_id
        LEFT JOIN surplus_capturing_jit_order_owners jit ON sc.id = jit.auction_id
        WHERE sc.id < $1
        ORDER BY sc.id DESC
        LIMIT $2;"#;

        sqlx::query_as(QUERY)
        .bind(auction_id)
        .bind(batch_size)
        .fetch_all(ex)
        .await
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Auction {
    pub id: i64,
    pub block: i64,
    pub deadline: i64,
    pub order_uids: Vec<OrderUid>,
    // External native prices
    pub price_tokens: Vec<Address>,
    pub price_values: Vec<BigDecimal>,
    pub surplus_capturing_jit_order_owners: Vec<Address>,
}

pub async fn save(ex: &mut PgConnection, auction: Auction) -> Result<(), sqlx::Error> {
    const QUERY: &str = r#"
INSERT INTO competition_auctions (id, block, deadline, order_uids, price_tokens, price_values, surplus_capturing_jit_order_owners)
VALUES ($1, $2, $3, $4, $5, $6, $7)
    ;"#;

    sqlx::query(QUERY)
        .bind(auction.id)
        .bind(auction.block)
        .bind(auction.deadline)
        .bind(auction.order_uids)
        .bind(auction.price_tokens)
        .bind(auction.price_values)
        .bind(auction.surplus_capturing_jit_order_owners)
        .execute(ex)
        .await?;

    Ok(())
}

pub fn u256_to_big_uint(input: &U256) -> BigUint {
    let mut bytes = [0; 32];
    input.to_big_endian(&mut bytes);
    BigUint::from_bytes_be(&bytes)
}

pub fn u256_to_big_decimal(u256: &U256) -> BigDecimal {
    let big_uint = u256_to_big_uint(u256);
    BigDecimal::from(BigInt::from(big_uint))
}
