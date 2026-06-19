This is a ticketing API i built across a day as a hobby, it's a small back-end service for booking event seats that wont oversell or accidentally books two seats at once, even if many try booking at the exact same time

I built this because i wanted to build a project to proove to myself that i can handle working with backend that actually can break on concurrency, anyone can write a booking endpoint that works for one person but i wanted to proove to myself that i can get past the hard question: "What if 200 book at once?"

I chose to use Rust because it's type system catches many concurrency mistakes before the code runs

It models events and individual seats, lets a client reserve a seat for an event over HTTP, guarantees a seat is booked for only one person (preventing 200 booking the same seat) and supports safe retries by an indempotency key (e.g. a network response that dropped wont buy 2 seats)

It works by using reserve_any_seat inside a single database transaction (all or nothing)

1. If a request carries 'Idempotency-Key' that was already booked under, it returns the original booking rather than making a new one making sure that if the first request suceeded while the response got lost client will retry instead of booking a second seat

2. it locks a free seat by running 'SELECT' & 'FOR UPDATE SKIP LOCKED' to grab the next available seat and lock it, 'FOR UPDATE' locking the row so others wont take it, 'SKIP LOCKED' meaning another request doesent queue behind a locked seat skipping to the next available one but if there is no seat to lock it assumes the event is sold out so it returns 409

3. it inserts the booking into the 'bookings' table, that table has a 'UNIQUE' constraint on the idempotency key so even if two identical requests both slip past step 1 at the exact same time only one of the inserts can actually win, the other one hits a unique violation error which it catches, rolls back and returns the booking that already won instead of erroring out, so a client can never end up with two seats

4. after the seat is locked and the booking is inserted it updates the seat to 'booked' and then commits the transaction, only after that commit does any of this become real and visible to everyone else, and if any step along the way fails the whole transaction rolls back so it's like nothing ever happened, meaning you never get a half finished booking or a seat stuck as taken with no booking behind it

on top of those steps i set the transaction to run at 'READ COMMITTED' on purpose, the safety here comes from the row lock in step 2 and not from the isolation level so i didn't need anything stronger, a level like 'SERIALIZABLE' wouldn't make it any safer it would just make postgres abort some transactions with errors that i'd have to catch and retry

the api itself is small, 'GET /health' is a basic liveness check, 'GET /ready' also checks that the database is reachable, and 'POST /events/:event_id/reservations' is the one that actually reserves a seat (you send an 'Idempotency-Key' header so a retry is safe)

it's built with Rust and Axum for the http layer, SQLx with PostgreSQL for the database, and Docker plus GitHub Actions for running it locally and for CI

for testing, the main test creates an event with 50 seats then fires 200 reservation attempts at the exact same time, it checks that exactly 50 succeed, 150 come back sold out and that no seat is ever booked twice, that last check is the real proof that the concurrency handling works, and CI runs it against a real postgres on every push

things i'd add next would be timed seat holds that expire so a seat isn't locked forever, letting a client pick a specific seat instead of any available one