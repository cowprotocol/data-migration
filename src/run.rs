use crate::{
    database::Postgres,
    database_solver_competition::{
        big_decimal_to_u256, fetch_batch, fetch_competition_order_execution, Auction, ByteArray,
    },
    solver_competition_api::SolverCompetitionDB,
};
use anyhow::{Context, Result};
use clap::Parser;
use primitive_types::H160;
use std::{num::NonZero, ops::DerefMut};

pub async fn start(args: impl Iterator<Item = String>) {
    let args = crate::arguments::Arguments::parse_from(args);

    let db = Postgres::new(args.db_url.as_str(), NonZero::new(500).unwrap())
        .await
        .unwrap();

    populate_historic_auctions(&db).await.unwrap();

    // sleep for 10 minutes
    std::thread::sleep(std::time::Duration::from_secs(600));
}

pub async fn populate_historic_auctions(db: &Postgres) -> Result<()> {
    println!("starting data migration for auction data");

    const BATCH_SIZE: i64 = 1;

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

    let starting_auction_number = current_auction_id;

    loop {
        println!(
            "populating historic auctions from auction {}, executed in percent: {}",
            current_auction_id,
            (starting_auction_number - current_auction_id) as f64 / starting_auction_number as f64
                * 100.0
        );

        // fetch the next batch of auctions
        let competitions = fetch_batch(&mut ex, current_auction_id, BATCH_SIZE).await;
        let Ok(competitions) = competitions else {
            // added because auction 3278851 has null json - unexpected entry in the database
            println!("failed to deserialize {}", current_auction_id);
            current_auction_id -= 1;
            continue;
        };

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

        // sleep for 50ms
        std::thread::sleep(std::time::Duration::from_millis(50));

        ex = db.pool.begin().await?;

        // update the current auction id
        current_auction_id = competitions.last().unwrap().id;
    }

    Ok(())
}

// Function to convert all rows in order_execution table, specifically the `executed_fee` column to be expressed in surplus token instead of the sell token
pub async fn convert_executed_fee(db: &Postgres) -> Result<()> {
    println!("starting data migration for conversion of executed fees");

    let mut ex = db.pool.begin().await?;

    // find entry in `solver_competition` with the lowest auction_id, as a
    // starting point
    let current_auction_id: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(id) FROM solver_competitions;")
            .fetch_one(ex.deref_mut())
            .await
            .context("fetch highest auction id")?;

    let Some(mut current_auction_id) = current_auction_id else {
        println!("solver_competitions is empty, nothing to process");
        return Ok(());
    };

    let starting_auction_number = current_auction_id;

    loop {
        println!(
            "populating from auction {}, executed in percent: {}",
            current_auction_id,
            (starting_auction_number - current_auction_id) as f64 / starting_auction_number as f64
                * 100.0
        );

        let competitions = fetch_competition_order_execution(&mut ex, current_auction_id, 1).await;
        let Ok(competitions) = competitions else {
            // added because auction 3278851 has null json - unexpected entry in the database
            println!("failed to deserialize {}", current_auction_id);
            current_auction_id -= 1;
            continue;
        };

        if competitions.is_empty() {
            println!("no more competitions to process");
            break;
        }

        println!("processing {} competitions", competitions.len());
        for solver_competition in &competitions {
            let competition: SolverCompetitionDB =
                serde_json::from_value(solver_competition.json.clone())
                    .context("deserialize SolverCompetitionDB")?;

            // find rows in order_execution table with auction_id = solver_competition.id
            let order_executions: Vec<crate::database_order_executions::OrderExecution> =
                crate::database_order_executions::fetch(&mut ex, solver_competition.id)
                    .await
                    .context("fetch order executions")?;

            // find orders for each order_execution
            let mut result = Vec::new();
            for order_execution in &order_executions {
                // find order in orders table with order_uid = order_execution.order_uid
                let order: Option<crate::database_orders::Order> =
                    crate::database_orders::fetch_from_orders(&mut ex, &order_execution.order_uid)
                        .await
                        .context("fetch order")?;
                match order {
                    Some(order) => {
                        result.push((order_execution, order));
                    }
                    None => {
                        // find order in jit_orders table with order_uid = order_execution.order_uid
                        let jit_order: Option<crate::database_orders::Order> =
                            crate::database_orders::fetch_from_jit_orders(
                                &mut ex,
                                &order_execution.order_uid,
                            )
                            .await
                            .context("fetch jit order")?;
                        match jit_order {
                            Some(jit_order) => {
                                result.push((order_execution, jit_order));
                            }
                            None => {
                                println!(
                                    "order not found for order_uid: {:?}, auction_id: {}",
                                    order_execution.order_uid, solver_competition.id
                                );
                            }
                        }
                    }
                }
            }

            for (order_execution, order) in &result {
                // fee needs to be updated for sell orders that have fee in sell token
                if order.kind == crate::database_orders::OrderKind::Sell
                    && order_execution.executed_fee_token == order.sell_token
                {
                    // update the executed_fee to be in buy token
                    let sell_token_price = competition
                        .solutions
                        .last()
                        .unwrap()
                        .clearing_prices
                        .get(&H160(order.sell_token.0));
                    let buy_token_price = competition
                        .solutions
                        .last()
                        .unwrap()
                        .clearing_prices
                        .get(&H160(order.buy_token.0));
                    let (sell_token_price, buy_token_price) =
                        match (sell_token_price, buy_token_price) {
                            (Some(sell_token_price), Some(buy_token_price)) => {
                                (sell_token_price, buy_token_price)
                            }
                            _ => {
                                println!(
                                    "prices not found for order_uid: {:?}, auction_id: {}",
                                    order_execution.order_uid, solver_competition.id
                                );
                                continue;
                            }
                        };

                    let executed_fee = big_decimal_to_u256(&order_execution.executed_fee).unwrap();

                    let fee_in_buy_token = executed_fee * sell_token_price / buy_token_price;

                    crate::database_order_executions::update(
                        &mut ex,
                        crate::database_order_executions::OrderExecution {
                            order_uid: order_execution.order_uid,
                            auction_id: order_execution.auction_id,
                            executed_fee: crate::database_solver_competition::u256_to_big_decimal(
                                &fee_in_buy_token,
                            ),
                            executed_fee_token: order.buy_token,
                        },
                    )
                    .await
                    .context("database_order_executions::update")?;
                }
            }
        }

        // commit each batch separately
        ex.commit().await?;

        // sleep for 50ms
        std::thread::sleep(std::time::Duration::from_millis(50));

        ex = db.pool.begin().await?;

        // update the current auction id
        current_auction_id = competitions.last().unwrap().id;
    }

    Ok(())
}

// pub async fn fix_missing_historic_auctions(db: &Postgres) -> Result<()> {
//     println!("starting data migration fix for auction data");

//     const BATCH_SIZE: i64 = 1;

//     let mut ex = db.pool.begin().await?;

//     // there is a gap of entries in `competition_auctions` that need to be filled

//     // we identify this gap by looking at the `solver_competitions` table

//     loop {
//         // fetch the next batch of auctions
//         let competitions = fetch_batch(&mut ex, BATCH_SIZE).await;
//         let Ok(competitions) = competitions else {
//             // added because auction 3278851 has null json - this is a one-off fix
//             println!("failed to deserialize");
//             continue;
//         };

//         if competitions.is_empty() {
//             println!("no more auctions to process");
//             break;
//         }

//         println!(
//             "processing {} auctions, first one {}",
//             competitions.len(),
//             competitions.last().map(|c| c.id).unwrap_or(0)
//         );

//         for solver_competition in &competitions {
//             let competition =
//                 serde_json::from_value::<SolverCompetitionDB>(solver_competition.json.clone())
//                     .context("deserialize SolverCompetitionDB");

//             let Ok(competition) = competition else {
//                 println!(
//                     "failed to deserialize SolverCompetitionDB, auction: {}",
//                     solver_competition.id
//                 );
//                 continue;
//             };

//             // populate historic auctions
//             let auction = Auction {
//                 id: solver_competition.id,
//                 block: i64::try_from(competition.auction_start_block).context("block overflow")?,
//                 deadline: solver_competition.deadline,
//                 order_uids: competition
//                     .auction
//                     .orders
//                     .iter()
//                     .map(|order| ByteArray(order.0))
//                     .collect(),
//                 price_tokens: competition
//                     .auction
//                     .prices
//                     .keys()
//                     .map(|token| ByteArray(token.0))
//                     .collect(),
//                 price_values: competition
//                     .auction
//                     .prices
//                     .values()
//                     .map(crate::database_solver_competition::u256_to_big_decimal)
//                     .collect(),
//                 surplus_capturing_jit_order_owners: solver_competition
//                     .surplus_capturing_jit_order_owners
//                     .clone(),
//             };

//             if let Err(err) = crate::database_solver_competition::save(&mut ex, auction).await {
//                 println!(
//                     "failed to save auction: {:?}, auction: {}",
//                     err, solver_competition.id
//                 );
//             }
//         }

//         // commit each batch separately
//         ex.commit().await?;

//         // sleep for 50ms
//         std::thread::sleep(std::time::Duration::from_millis(50));

//         ex = db.pool.begin().await?;
//     }

//     Ok(())
// }
