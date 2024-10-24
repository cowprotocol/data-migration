use url::Url;

#[derive(clap::Parser)]
pub struct Arguments {
    /// Url of the Postgres database. By default connects to locally running
    /// postgres.
    #[clap(long, env, default_value = "postgresql://")]
    pub db_url: Url,
}
