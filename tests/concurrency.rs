use sqlx::postgres::PgPoolOptions;
use ticketing_api::error::AppError;
use ticketing_api::repo;
use uuid::Uuid;

async fn setup_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL not set");
            return None;
        }
    };

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&url)
        .await
        .expect("connect to postgres");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    Some(pool)
}

#[tokio::test]
async fn does_not_oversell_under_concurrency() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let seat_count: i64 = 50;
    let attempts: i64 = 200;

    let (event_id,): (Uuid,) = sqlx::query_as("INSERT INTO events (name) VALUES ($1) RETURNING id")
        .bind("load-test")
        .fetch_one(&pool)
        .await
        .unwrap();

    for i in 0..seat_count {
        sqlx::query("INSERT INTO seats (event_id, label) VALUES ($1, $2)")
            .bind(event_id)
            .bind(format!("S{i:03}"))
            .execute(&pool)
            .await
            .unwrap();
    }

    let mut handles = Vec::new();
    for i in 0..attempts {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            repo::reserve_any_seat(&pool, event_id, &format!("user-{i}"), None).await
        }));
    }

    let mut booked = 0;
    let mut sold_out = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => booked += 1,
            Err(AppError::SoldOut) => sold_out += 1,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    assert_eq!(booked, seat_count, "exactly every seat should be sold once");
    assert_eq!(sold_out, attempts - seat_count, "the rest must be rejected");

    let (rows_booked,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM seats WHERE event_id = $1 AND status = 'booked'")
            .bind(event_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(rows_booked, seat_count, "no seat may be booked twice");
}
