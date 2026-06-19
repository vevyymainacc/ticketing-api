drop index if exists idx_seats_event_status;

create index if not exists idx_seats_event_available
    on seats (event_id, label) where status = 'available';
