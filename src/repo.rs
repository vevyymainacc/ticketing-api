use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

pub struct ReservedSeat {
    pub booking_id: Uuid,
    pub seat_id: Uuid,
    pub seat_label: String,
}

pub async fn reserve_any_seat(
    pool: &PgPool,
    event_id: Uuid,
    user_ref: &str,
    idempotency_key: Option<&str>,
) -> Result<ReservedSeat, AppError> {
    // done so if a client retries using the same key (e.g. if they time out) it returns the original booking rather than reserving a second seat
    if let Some(key) = idempotency_key {
        if let Some(existing) = find_booking_by_key(pool, key).await? {
            return Ok(existing);
        }
    }

    let mut tx = pool.begin().await?;

    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;

    let event: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM events WHERE id = $1")
        .bind(event_id)
        .fetch_optional(&mut *tx)
        .await?;
    if event.is_none() {
        return Err(AppError::EventNotFound);
    }

    // for update skip locked means two concurrent requests always grab different seats preventing double booking withouth blocking each other
    let seat: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, label
        FROM seats
        WHERE event_id = $1 AND status = 'available'
        ORDER BY label
        FOR UPDATE SKIP LOCKED
        LIMIT 1
        "#,
    )
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (seat_id, seat_label) = seat.ok_or(AppError::SoldOut)?;

    let insert = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO bookings (event_id, seat_id, user_ref, idempotency_key)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(event_id)
    .bind(seat_id)
    .bind(user_ref)
    .bind(idempotency_key)
    .fetch_one(&mut *tx)
    .await;

    // chose an unique constraint because its enforce at db level so it prevents a second identical insert from ever committing, meaning if 2 requests happen at the same time only one is ever created
    let booking_id = match insert {
        Ok(row) => row.0,
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            tx.rollback().await?;
            if let Some(key) = idempotency_key {
                if let Some(existing) = find_booking_by_key(pool, key).await? {
                    return Ok(existing);
                }
            }
            return Err(AppError::Db(sqlx::Error::Database(db)));
        }
        Err(err) => return Err(AppError::Db(err)),
    };

    sqlx::query("UPDATE seats SET status = 'booked', booking_id = $1 WHERE id = $2")
        .bind(booking_id)
        .bind(seat_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(ReservedSeat {
        booking_id,
        seat_id,
        seat_label,
    })
}

async fn find_booking_by_key(pool: &PgPool, key: &str) -> Result<Option<ReservedSeat>, AppError> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        r#"
        SELECT b.id, b.seat_id, s.label
        FROM bookings b
        JOIN seats s ON s.id = b.seat_id
        WHERE b.idempotency_key = $1
        "#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(booking_id, seat_id, seat_label)| ReservedSeat {
        booking_id,
        seat_id,
        seat_label,
    }))
}

pub struct CreatedEvent {
    pub event_id: Uuid,
    pub name: String,
    pub seat_count: i64,
}

pub async fn create_event(
    pool: &PgPool,
    name: &str,
    seat_count: i32,
) -> Result<CreatedEvent, AppError> {
    if seat_count <= 0 {
        return Err(AppError::Invalid("seat_count must be greater than zero"));
    }
    if seat_count > 100_000 {
        return Err(AppError::Invalid("seat_count must be 100000 or fewer"));
    }

    let mut tx = pool.begin().await?;

    let (event_id,): (Uuid,) = sqlx::query_as("INSERT INTO events (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;

    let result = sqlx::query(
        r#"
        INSERT INTO seats (event_id, label)
        SELECT $1, 'S' || lpad(g::text, 4, '0')
        FROM generate_series(1, $2) AS g
        "#,
    )
    .bind(event_id)
    .bind(seat_count)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(CreatedEvent {
        event_id,
        name: name.to_string(),
        seat_count: result.rows_affected() as i64,
    })
}
