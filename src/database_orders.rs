use sqlx::{
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueFormat, PgValueRef},
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, sqlx::Type)]
#[sqlx(type_name = "OrderKind")]
#[sqlx(rename_all = "lowercase")]
pub enum OrderKind {
    #[default]
    Buy,
    Sell,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Order {
    pub sell_token: Address,
    pub buy_token: Address,
    pub kind: OrderKind,
}

pub async fn fetch_from_orders(
    ex: &mut PgConnection,
    order_uid: &OrderUid,
) -> Result<Option<Order>, sqlx::Error> {
    const QUERY: &str = r#"
        SELECT sell_token, buy_token, kind
        FROM orders
        WHERE uid = $1;"#;

    sqlx::query_as(QUERY)
        .bind(order_uid)
        .fetch_optional(ex)
        .await
}

pub async fn fetch_from_jit_orders(
    ex: &mut PgConnection,
    order_uid: &OrderUid,
) -> Result<Option<Order>, sqlx::Error> {
    const QUERY: &str = r#"
        SELECT sell_token, buy_token, kind
        FROM jit_orders
        WHERE uid = $1;"#;

    sqlx::query_as(QUERY)
        .bind(order_uid)
        .fetch_optional(ex)
        .await
}
