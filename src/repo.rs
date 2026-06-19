use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

pub struct BookingRecord {
    pub booking_id: Uuid,
    pub event_id: Uuid,
    pub seat_id: Uuid,
    pub seat_label: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn reserve_any_seat(
    pool: &PgPool,
    event_id: Uuid,
    user_ref: &str,
    idempotency_key: &str,
    hold_ttl_secs: i64,
) -> Result<BookingRecord, AppError> {
    // done so if a client retries using the same key (e.g. if they time out) it returns the original booking rather than reserving a second seat
    if let Some(existing) = find_booking(pool, idempotency_key).await? {
        return Ok(existing);
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
        return Err(AppError::NotFound("event not found"));
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
            if let Some(existing) = find_booking(pool, idempotency_key).await? {
                return Ok(existing);
            }
            return Err(AppError::Db(sqlx::Error::Database(db)));
        }
        Err(err) => return Err(AppError::Db(err)),
    };

    let (held_until,): (DateTime<Utc>,) = sqlx::query_as(
        r#"
        UPDATE seats
        SET status = 'held', booking_id = $1, held_until = now() + make_interval(secs => $2)
        WHERE id = $3
        RETURNING held_until
        "#,
    )
    .bind(booking_id)
    .bind(hold_ttl_secs as f64)
    .bind(seat_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(BookingRecord {
        booking_id,
        event_id,
        seat_id,
        seat_label,
        status: "held".to_string(),
        expires_at: Some(held_until),
    })
}

pub async fn confirm_reservation(
    pool: &PgPool,
    idempotency_key: &str,
) -> Result<BookingRecord, AppError> {
    let booking = find_booking(pool, idempotency_key)
        .await?
        .ok_or(AppError::NotFound("booking not found"))?;

    if booking.status == "booked" {
        return Ok(booking);
    }
    if booking.status == "expired" {
        return Err(AppError::Conflict("hold has expired"));
    }

    let mut tx = pool.begin().await?;

    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;

    // i locked the seat row here so no other transaction can sneak in
    let row: Option<(String, Option<Uuid>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT status, booking_id, held_until FROM seats WHERE id = $1 FOR UPDATE")
            .bind(booking.seat_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (status, seat_booking_id, held_until) =
        row.ok_or(AppError::NotFound("booking not found"))?;

    let owned = seat_booking_id == Some(booking.booking_id);
    if status == "booked" && owned {
        tx.commit().await?;
        return Ok(BookingRecord {
            status: "booked".to_string(),
            expires_at: None,
            ..booking
        });
    }

    let still_held = owned && status == "held" && held_until.is_some_and(|t| t >= Utc::now());
    if !still_held {
        return Err(AppError::Conflict("hold has expired"));
    }

    sqlx::query("UPDATE seats SET status = 'booked', held_until = NULL WHERE id = $1")
        .bind(booking.seat_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(BookingRecord {
        status: "booked".to_string(),
        expires_at: None,
        ..booking
    })
}

type BookingRow = (Uuid, Uuid, Uuid, String, String, Option<DateTime<Utc>>);

pub async fn find_booking(
    pool: &PgPool,
    idempotency_key: &str,
) -> Result<Option<BookingRecord>, AppError> {
    let row: Option<BookingRow> = sqlx::query_as(
        r#"
        SELECT
            b.id,
            b.event_id,
            b.seat_id,
            s.label,
            CASE
                WHEN s.booking_id = b.id AND s.status = 'booked' THEN 'booked'
                WHEN s.booking_id = b.id AND s.status = 'held' AND s.held_until >= now() THEN 'held'
                ELSE 'expired'
            END,
            CASE
                WHEN s.booking_id = b.id AND s.status = 'held' AND s.held_until >= now()
                THEN s.held_until
                ELSE NULL
            END
        FROM bookings b
        JOIN seats s ON s.id = b.seat_id
        WHERE b.idempotency_key = $1
        "#,
    )
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(booking_id, event_id, seat_id, seat_label, status, expires_at)| BookingRecord {
            booking_id,
            event_id,
            seat_id,
            seat_label,
            status,
            expires_at,
        },
    ))
}

pub async fn sweep_expired_holds(pool: &PgPool) -> Result<u64, AppError> {
    let result = sqlx::query(
        r#"
        UPDATE seats
        SET status = 'available', held_until = NULL, booking_id = NULL
        WHERE status = 'held' AND held_until < now()
        "#,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
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
        SELECT $1, 'S' || lpad(g::text, 6, '0')
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

pub struct EventAvailability {
    pub event_id: Uuid,
    pub total: i64,
    pub available: i64,
    pub held: i64,
    pub booked: i64,
}

pub async fn event_availability(
    pool: &PgPool,
    event_id: Uuid,
) -> Result<Option<EventAvailability>, AppError> {
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM events WHERE id = $1")
        .bind(event_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Ok(None);
    }

    let (total, available, held, booked): (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            count(*),
            count(*) FILTER (WHERE status = 'available' OR (status = 'held' AND held_until < now())),
            count(*) FILTER (WHERE status = 'held' AND held_until >= now()),
            count(*) FILTER (WHERE status = 'booked')
        FROM seats
        WHERE event_id = $1
        "#,
    )
    .bind(event_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(EventAvailability {
        event_id,
        total,
        available,
        held,
        booked,
    }))
}
