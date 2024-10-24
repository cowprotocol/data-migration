use crate::{
    database::Postgres,
    database_solver_competition::{fetch_batch, Auction, ByteArray, RichSolverCompetition},
    solver_competition_api::SolverCompetitionDB,
};
use anyhow::{Context, Result};
use clap::Parser;
use std::{num::NonZero, ops::DerefMut};

pub async fn start(args: impl Iterator<Item = String>) {
    let args = crate::arguments::Arguments::parse_from(args);

    let db = Postgres::new(args.db_url.as_str(), NonZero::new(500).unwrap())
        .await
        .unwrap();

    // test db connection
    let mut ex = db.pool.begin().await.unwrap();
    let current_auction_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MIN(id) FROM competition_auctions;")
            .fetch_one(ex.deref_mut())
            .await
            .context("fetch lowest auction id")
            .unwrap();
    println!("current auction id: {:?}", current_auction_id);

    // sleep for 10 minutes
    std::thread::sleep(std::time::Duration::from_secs(600));
}

pub async fn populate_historic_auctions(db: &Postgres) -> Result<()> {
    const BATCH_SIZE: i64 = 50;

    let mut ex = db.pool.begin().await?;

    // find entry in `competition_auctions` with the lowest auction_id, as a
    // starting point
    let current_auction_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MIN(id) FROM competition_auctions;")
            .fetch_one(ex.deref_mut())
            .await
            .context("fetch lowest auction id")?;

    let Some(mut current_auction_id) = current_auction_id else {
        println!("competition_auctions is empty, nothing to process");
        return Ok(());
    };

    loop {
        println!(
            "populating historic auctions from auction {}",
            current_auction_id
        );

        // fetch the next batch of auctions
        let competitions: Vec<RichSolverCompetition> =
            fetch_batch(&mut ex, current_auction_id, BATCH_SIZE).await?;

        if competitions.is_empty() {
            println!("no more auctions to process");
            break;
        }

        println!("processing {} auctions", competitions.len());

        for solver_competition in &competitions {
            let competition: SolverCompetitionDB =
                serde_json::from_value(solver_competition.json.clone())
                    .context("deserialize SolverCompetitionDB")?;

            // populate historic auctions
            let auction = Auction {
                id: solver_competition.id,
                block: i64::try_from(competition.auction_start_block).context("block overflow")?,
                deadline: solver_competition.deadline,
                order_uids: competition
                    .auction
                    .orders
                    .iter()
                    .map(|order| ByteArray(order.0))
                    .collect(),
                price_tokens: competition
                    .auction
                    .prices
                    .keys()
                    .map(|token| ByteArray(token.0))
                    .collect(),
                price_values: competition
                    .auction
                    .prices
                    .values()
                    .map(crate::database_solver_competition::u256_to_big_decimal)
                    .collect(),
                surplus_capturing_jit_order_owners: solver_competition
                    .surplus_capturing_jit_order_owners
                    .clone(),
            };

            if let Err(err) = crate::database_solver_competition::save(&mut ex, auction).await {
                println!(
                    "failed to save auction: {:?}, auction: {}",
                    err, solver_competition.id
                );
            }
        }

        // commit each batch separately
        ex.commit().await?;
        ex = db.pool.begin().await?;

        // update the current auction id
        current_auction_id = competitions.last().unwrap().id;
    }

    Ok(())
}
