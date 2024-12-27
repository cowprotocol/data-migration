use bigdecimal::BigDecimal;
use sqlx::{
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueFormat, PgValueRef},
    Decode, Encode, PgConnection, Postgres, Type,
};
use std::fmt::{self, Debug, Formatter};

use crate::database_orders::{Address, OrderUid};

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

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OrderExecution {
    pub order_uid: OrderUid,
    pub auction_id: i64,
    pub executed_fee: BigDecimal,
    pub executed_fee_token: Address,
}

pub async fn fetch(
    ex: &mut PgConnection,
    auction_id: i64,
) -> Result<Vec<OrderExecution>, sqlx::Error> {
    const QUERY: &str = r#"
        SELECT order_uid, auction_id, executed_fee, executed_fee_token
        FROM order_execution
        WHERE auction_id = $1;"#;

    sqlx::query_as(QUERY).bind(auction_id).fetch_all(ex).await
}

pub async fn update(
    ex: &mut PgConnection,
    order_execution: OrderExecution,
) -> Result<(), sqlx::Error> {
    // update existing row in order execution (primary key being order_uid + auction_id) with new values of executed fee and executed fee token
    const QUERY: &str = r#"
        UPDATE order_execution
        SET executed_fee = $1, executed_fee_token = $2
        WHERE order_uid = $3 AND auction_id = $4;"#;

    sqlx::query(QUERY)
        .bind(order_execution.executed_fee)
        .bind(order_execution.executed_fee_token)
        .bind(order_execution.order_uid)
        .bind(order_execution.auction_id)
        .execute(ex)
        .await?;

    Ok(())
}
