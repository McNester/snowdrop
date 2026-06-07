CREATE TABLE users (
  id BIGINT PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(128) NOT NULL,
  password_hash VARCHAR(128) NOT NULL,
  role VARCHAR(32) NOT NULL DEFAULT 'user',
  created_at TIMESTAMP NOT NULL
);

CREATE TABLE airports (
  id BIGINT PRIMARY KEY,
  airport_code VARCHAR(8) NOT NULL UNIQUE,
  city VARCHAR(64) NOT NULL,
  country VARCHAR(64) NOT NULL
);

CREATE TABLE airlines (
  id BIGINT PRIMARY KEY,
  airline_code VARCHAR(8) NOT NULL UNIQUE,
  name VARCHAR(96) NOT NULL
);

CREATE TABLE aircraft (
  id BIGINT PRIMARY KEY,
  aircraft_code VARCHAR(16) NOT NULL UNIQUE,
  model VARCHAR(96) NOT NULL,
  seat_capacity INT NOT NULL
);

CREATE TABLE flights (
  id BIGINT PRIMARY KEY,
  flight_number VARCHAR(24) NOT NULL UNIQUE,
  airline_id BIGINT NOT NULL,
  aircraft_id BIGINT NOT NULL,
  from_airport_id BIGINT NOT NULL,
  to_airport_id BIGINT NOT NULL,
  from_city VARCHAR(64) NOT NULL,
  to_city VARCHAR(64) NOT NULL,
  airline_name VARCHAR(96) NOT NULL,
  aircraft_model VARCHAR(96) NOT NULL,
  depart_at TIMESTAMP NOT NULL,
  arrive_at TIMESTAMP NOT NULL,
  depart_day DATE NOT NULL,
  price NUMERIC(10,2) NOT NULL,
  seats_available INT NOT NULL,
  flight_status VARCHAR(16) NOT NULL DEFAULT 'scheduled',
  created_at TIMESTAMP NOT NULL,
  FOREIGN KEY (airline_id) REFERENCES airlines(id),
  FOREIGN KEY (aircraft_id) REFERENCES aircraft(id),
  FOREIGN KEY (from_airport_id) REFERENCES airports(id),
  FOREIGN KEY (to_airport_id) REFERENCES airports(id)
);

CREATE TABLE bookings (
  id BIGINT PRIMARY KEY,
  user_id BIGINT NOT NULL,
  flight_id BIGINT NOT NULL,
  passenger_name VARCHAR(128) NOT NULL,
  passenger_email VARCHAR(255) NOT NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'confirmed',
  total_price NUMERIC(10,2) NOT NULL,
  flight_number VARCHAR(24) NOT NULL,
  from_city VARCHAR(64) NOT NULL,
  to_city VARCHAR(64) NOT NULL,
  created_at TIMESTAMP NOT NULL
);

CREATE TABLE flight_search_events (
  id BIGINT PRIMARY KEY,
  user_id BIGINT,
  from_city VARCHAR(64) NOT NULL,
  to_city VARCHAR(64) NOT NULL,
  event_kind VARCHAR(32) NOT NULL,
  event_day DATE NOT NULL,
  created_at TIMESTAMP NOT NULL,
  result_count INT NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE TABLE route_daily_stats (
  id BIGINT PRIMARY KEY,
  stat_day DATE NOT NULL,
  from_city VARCHAR(64) NOT NULL,
  to_city VARCHAR(64) NOT NULL,
  search_count INT NOT NULL,
  booking_count INT NOT NULL,
  revenue NUMERIC(12,2) NOT NULL
);
