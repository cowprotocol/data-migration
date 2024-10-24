#[tokio::main]
async fn main() {
    data_migration::run::start(std::env::args()).await;
}
