use {sqlx::PgPool, std::num::NonZeroUsize};

#[derive(Debug, Clone)]
pub struct Config {
    pub insert_batch_size: NonZeroUsize,
}

#[derive(Debug, Clone)]
pub struct Postgres {
    pub pool: PgPool,
    pub config: Config,
}

impl Postgres {
    pub async fn new(url: &str, insert_batch_size: NonZeroUsize) -> sqlx::Result<Self> {
        Ok(Self {
            pool: PgPool::connect(url).await?,
            config: Config { insert_batch_size },
        })
    }

    pub async fn with_defaults() -> sqlx::Result<Self> {
        Self::new("postgresql://", NonZeroUsize::new(500).unwrap()).await
    }
}
