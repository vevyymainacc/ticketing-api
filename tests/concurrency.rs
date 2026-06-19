use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
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

async fn seed_event(pool: &sqlx::PgPool, seats: i64) -> Uuid {
    let (event_id,): (Uuid,) = sqlx::query_as("INSERT INTO events (name) VALUES ($1) RETURNING id")
        .bind("test-event")
        .fetch_one(pool)
        .await
        .unwrap();

    for i in 0..seats {
        sqlx::query("INSERT INTO seats (event_id, label) VALUES ($1, $2)")
            .bind(event_id)
            .bind(format!("S{i:04}"))
            .execute(pool)
            .await
            .unwrap();
    }

    event_id
}

async fn count_status(pool: &sqlx::PgPool, event_id: Uuid, status: &str) -> i64 {
    let (rows,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM seats WHERE event_id = $1 AND status = $2")
            .bind(event_id)
            .bind(status)
            .fetch_one(pool)
            .await
            .unwrap();
    rows
}

#[tokio::test]
async fn does_not_oversell_under_concurrency() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let seat_count: i64 = 50;
    let attempts: i64 = 200;
    let event_id = seed_event(&pool, seat_count).await;

    let mut handles = Vec::new();
    for i in 0..attempts {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let key = format!("key-{event_id}-{i}");
            repo::reserve_any_seat(&pool, event_id, &format!("user-{i}"), &key, 120).await
        }));
    }

    let mut held = 0;
    let mut sold_out = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => held += 1,
            Err(AppError::SoldOut) => sold_out += 1,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    assert_eq!(held, seat_count, "exactly every seat should be held once");
    assert_eq!(sold_out, attempts - seat_count, "the rest must be rejected");
    assert_eq!(
        count_status(&pool, event_id, "held").await,
        seat_count,
        "no seat may be held twice"
    );
}

#[tokio::test]
async fn repeated_key_returns_same_booking() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let event_id = seed_event(&pool, 10).await;
    let key = Uuid::new_v4().to_string();

    let first = repo::reserve_any_seat(&pool, event_id, "alice", &key, 120)
        .await
        .unwrap();
    let second = repo::reserve_any_seat(&pool, event_id, "alice", &key, 120)
        .await
        .unwrap();

    assert_eq!(
        first.booking_id, second.booking_id,
        "same key, same booking"
    );
    assert_eq!(first.seat_id, second.seat_id, "same key, same seat");
    assert_eq!(
        count_status(&pool, event_id, "held").await,
        1,
        "a repeated key must not consume a second seat"
    );
}

#[tokio::test]
async fn concurrent_requests_with_same_key_book_once() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let event_id = seed_event(&pool, 100).await;
    let key = Uuid::new_v4().to_string();
    let attempts = 30;

    let mut handles = Vec::new();
    for _ in 0..attempts {
        let pool = pool.clone();
        let key = key.clone();
        handles.push(tokio::spawn(async move {
            repo::reserve_any_seat(&pool, event_id, "alice", &key, 120).await
        }));
    }

    let mut booking_ids = std::collections::HashSet::new();
    for handle in handles {
        let reserved = handle
            .await
            .unwrap()
            .expect("idempotent reserve should succeed");
        booking_ids.insert(reserved.booking_id);
    }

    assert_eq!(
        booking_ids.len(),
        1,
        "all requests with the same key must resolve to one booking"
    );
    assert_eq!(
        count_status(&pool, event_id, "held").await,
        1,
        "the shared key must consume exactly one seat"
    );
}

#[tokio::test]
async fn confirm_turns_hold_into_booking() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let event_id = seed_event(&pool, 5).await;
    let key = Uuid::new_v4().to_string();

    let held = repo::reserve_any_seat(&pool, event_id, "alice", &key, 120)
        .await
        .unwrap();
    assert_eq!(held.status, "held");
    assert_eq!(count_status(&pool, event_id, "held").await, 1);

    let confirmed = repo::confirm_reservation(&pool, &key).await.unwrap();
    assert_eq!(confirmed.status, "booked");
    assert_eq!(confirmed.seat_id, held.seat_id);
    assert_eq!(count_status(&pool, event_id, "booked").await, 1);
    assert_eq!(count_status(&pool, event_id, "held").await, 0);

    let again = repo::confirm_reservation(&pool, &key).await.unwrap();
    assert_eq!(again.status, "booked", "confirming twice is idempotent");
}

#[tokio::test]
async fn expired_hold_is_reclaimed() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let event_id = seed_event(&pool, 1).await;
    let key = Uuid::new_v4().to_string();

    let held = repo::reserve_any_seat(&pool, event_id, "alice", &key, 1)
        .await
        .unwrap();
    assert_eq!(held.status, "held");
    assert_eq!(count_status(&pool, event_id, "available").await, 0);

    tokio::time::sleep(Duration::from_secs(2)).await;
    let reclaimed = repo::sweep_expired_holds(&pool).await.unwrap();
    assert!(reclaimed >= 1, "the expired hold should be swept back");

    assert_eq!(
        count_status(&pool, event_id, "available").await,
        1,
        "the seat is bookable again after expiry"
    );
    assert_eq!(count_status(&pool, event_id, "held").await, 0);
}

#[tokio::test]
async fn create_event_handles_large_seat_counts() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let count: i32 = 20_000;
    let created = repo::create_event(&pool, "big", count).await.unwrap();

    assert_eq!(
        created.seat_count, count as i64,
        "every seat must get a unique label even past 9999"
    );
}

#[tokio::test]
async fn availability_reflects_holds_and_bookings() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let event_id = seed_event(&pool, 10).await;
    let confirm_key = Uuid::new_v4().to_string();

    repo::reserve_any_seat(&pool, event_id, "u", &confirm_key, 120)
        .await
        .unwrap();
    repo::reserve_any_seat(&pool, event_id, "u", &Uuid::new_v4().to_string(), 120)
        .await
        .unwrap();
    repo::reserve_any_seat(&pool, event_id, "u", &Uuid::new_v4().to_string(), 120)
        .await
        .unwrap();
    repo::confirm_reservation(&pool, &confirm_key)
        .await
        .unwrap();

    let availability = repo::event_availability(&pool, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(availability.total, 10);
    assert_eq!(availability.booked, 1);
    assert_eq!(availability.held, 2);
    assert_eq!(availability.available, 7);

    assert!(repo::event_availability(&pool, Uuid::new_v4())
        .await
        .unwrap()
        .is_none());
}
