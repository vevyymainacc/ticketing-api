alter table seats add column if not exists held_until timestamptz;

alter table seats drop constraint if exists seats_status_check;

alter table seats add constraint seats_status_check
    check (status in ('available', 'held', 'booked'));
