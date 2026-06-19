create table if not exists events (
    id          uuid primary key default gen_random_uuid(),
    name        text not null,
    created_at  timestamptz not null default now()
);

create table if not exists seats (
    id          uuid primary key default gen_random_uuid(),
    event_id    uuid not null references events (id) on delete cascade,
    label       text not null,
    status      text not null default 'available'
        check (status in ('available', 'booked')),
    booking_id  uuid,
    unique (event_id, label)
);

create table if not exists bookings (
    id               uuid primary key default gen_random_uuid(),
    event_id         uuid not null references events (id) on delete cascade,
    seat_id          uuid not null references seats (id) on delete cascade,
    user_ref         text not null,
    idempotency_key  text,
    created_at       timestamptz not null default now(),
    unique (idempotency_key)
);

create index if not exists idx_seats_event_status
    on seats (event_id, status);
