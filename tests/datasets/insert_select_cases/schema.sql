CREATE TABLE live_orders (id BIGINT PRIMARY KEY, customer VARCHAR(64), total FLOAT, status VARCHAR(16));
CREATE TABLE archive_orders (id BIGINT PRIMARY KEY, customer VARCHAR(64), total FLOAT, status VARCHAR(16));
CREATE TABLE customer_totals (customer VARCHAR(64) PRIMARY KEY, total FLOAT);
