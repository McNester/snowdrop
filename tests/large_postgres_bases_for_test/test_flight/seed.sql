INSERT INTO users (id, email, full_name, password_hash, role, created_at)
SELECT id,
       CASE WHEN id = 1 THEN 'demo@testflight.local' ELSE 'user' || id || '@testflight.local' END,
       CASE WHEN id = 1 THEN 'Demo Passenger' ELSE 'Passenger ' || id END,
       '$2b$08$7jMHBOuggr3XFQtTkKBU5uC/1joXOItYSp1qoJTx0I1AIsFZxEQ0q',
       CASE WHEN id % 97 = 0 THEN 'admin' ELSE 'user' END,
       TIMESTAMP '2025-01-01 08:00:00' + (id % 365) * INTERVAL '1 day'
FROM generate_series(1, 5000) AS id;

INSERT INTO airports (id, airport_code, city, country) VALUES
(1, 'KBP', 'Kyiv', 'Ukraine'),
(2, 'WAW', 'Warsaw', 'Poland'),
(3, 'BER', 'Berlin', 'Germany'),
(4, 'LHR', 'London', 'United Kingdom'),
(5, 'CDG', 'Paris', 'France'),
(6, 'FCO', 'Rome', 'Italy'),
(7, 'MAD', 'Madrid', 'Spain'),
(8, 'VIE', 'Vienna', 'Austria'),
(9, 'PRG', 'Prague', 'Czechia'),
(10, 'AMS', 'Amsterdam', 'Netherlands'),
(11, 'BCN', 'Barcelona', 'Spain'),
(12, 'ZRH', 'Zurich', 'Switzerland'),
(13, 'OSL', 'Oslo', 'Norway'),
(14, 'ARN', 'Stockholm', 'Sweden'),
(15, 'HEL', 'Helsinki', 'Finland'),
(16, 'ATH', 'Athens', 'Greece'),
(17, 'LIS', 'Lisbon', 'Portugal'),
(18, 'DUB', 'Dublin', 'Ireland'),
(19, 'CPH', 'Copenhagen', 'Denmark'),
(20, 'BUD', 'Budapest', 'Hungary');

INSERT INTO airlines (id, airline_code, name) VALUES
(1, 'TF', 'Test Flight Air'),
(2, 'SNW', 'Snowdrop Airways'),
(3, 'PGX', 'Postgres Express'),
(4, 'KZ', 'Qyzylorda Wings'),
(5, 'EU', 'Euro Shuttle'),
(6, 'SKY', 'Skyline Connect'),
(7, 'NRT', 'Northern Routes'),
(8, 'SUN', 'Sunline Europe'),
(9, 'JET', 'Jetbridge'),
(10, 'AIR', 'Air Demo');

INSERT INTO aircraft (id, aircraft_code, model, seat_capacity)
SELECT id,
       'AC' || id,
       CASE id % 5
         WHEN 0 THEN 'Airbus A320'
         WHEN 1 THEN 'Boeing 737'
         WHEN 2 THEN 'Embraer E190'
         WHEN 3 THEN 'Airbus A321'
         ELSE 'Boeing 787'
       END,
       CASE id % 5
         WHEN 0 THEN 180
         WHEN 1 THEN 168
         WHEN 2 THEN 110
         WHEN 3 THEN 220
         ELSE 290
       END
FROM generate_series(1, 50) AS id;

INSERT INTO flights (
  id, flight_number, airline_id, aircraft_id, from_airport_id, to_airport_id,
  from_city, to_city, airline_name, aircraft_model,
  depart_at, arrive_at, depart_day, price, seats_available, flight_status, created_at
)
SELECT gs.id,
       'TF' || LPAD(gs.id::text, 7, '0'),
       al.id,
       ac.id,
       fa.id,
       ta.id,
       fa.city,
       ta.city,
       al.name,
       ac.model,
       TIMESTAMP '2026-06-01 06:00:00' + (gs.id % 120) * INTERVAL '1 day' + (gs.id % 18) * INTERVAL '1 hour',
       TIMESTAMP '2026-06-01 08:00:00' + (gs.id % 120) * INTERVAL '1 day' + (gs.id % 18) * INTERVAL '1 hour',
       DATE '2026-06-01' + (gs.id % 120),
       (80 + (gs.id % 900) * 0.75)::NUMERIC(10,2),
       20 + (gs.id % 180),
       CASE WHEN gs.id % 41 = 0 THEN 'delayed' ELSE 'scheduled' END,
       TIMESTAMP '2026-01-01 00:00:00' + (gs.id % 180) * INTERVAL '1 day'
FROM generate_series(1, 20000) AS gs(id)
JOIN airlines al ON al.id = ((gs.id - 1) % 10) + 1
JOIN aircraft ac ON ac.id = ((gs.id - 1) % 50) + 1
JOIN airports fa ON fa.id = ((gs.id - 1) % 20) + 1
JOIN airports ta ON ta.id = (gs.id % 20) + 1;

INSERT INTO bookings (
  id, user_id, flight_id, passenger_name, passenger_email, status, total_price,
  flight_number, from_city, to_city, created_at
)
SELECT gs.id,
       ((gs.id - 1) % 5000) + 1,
       f.id,
       'Passenger ' || gs.id,
       'passenger' || gs.id || '@testflight.local',
       CASE WHEN gs.id % 23 = 0 THEN 'cancelled' ELSE 'confirmed' END,
       f.price,
       f.flight_number,
       f.from_city,
       f.to_city,
       TIMESTAMP '2026-03-01 09:00:00' + (gs.id % 100) * INTERVAL '1 day' + (gs.id % 24) * INTERVAL '1 hour'
FROM generate_series(1, 2000) AS gs(id)
JOIN flights f ON f.id = ((gs.id * 7 - 1) % 20000) + 1;

INSERT INTO flight_search_events (
  id, user_id, from_city, to_city, event_kind, event_day, created_at, result_count
)
SELECT gs.id,
       CASE WHEN gs.id % 11 = 0 THEN NULL ELSE ((gs.id - 1) % 5000) + 1 END,
       fa.city,
       ta.city,
       CASE WHEN gs.id % 9 = 0 THEN 'details_opened' ELSE 'search' END,
       DATE '2026-05-01' + (gs.id % 90),
       TIMESTAMP '2026-05-01 06:00:00' + (gs.id % 90) * INTERVAL '1 day' + (gs.id % 24) * INTERVAL '1 hour',
       10 + (gs.id % 80)
FROM generate_series(1, 50000) AS gs(id)
JOIN airports fa ON fa.id = ((gs.id - 1) % 20) + 1
JOIN airports ta ON ta.id = (gs.id % 20) + 1;

INSERT INTO route_daily_stats (
  id, stat_day, from_city, to_city, search_count, booking_count, revenue
)
SELECT gs.id,
       DATE '2026-05-01' + (gs.id % 90),
       fa.city,
       ta.city,
       100 + (gs.id % 5000),
       5 + (gs.id % 160),
       ((5 + (gs.id % 160)) * (120 + (gs.id % 420)))::NUMERIC(12,2)
FROM generate_series(1, 3600) AS gs(id)
JOIN airports fa ON fa.id = ((gs.id - 1) % 20) + 1
JOIN airports ta ON ta.id = (gs.id % 20) + 1;
