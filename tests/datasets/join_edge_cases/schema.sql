CREATE TABLE customers (
  id BIGINT PRIMARY KEY,
  name VARCHAR(64)
);

CREATE TABLE orders (
  id BIGINT PRIMARY KEY,
  customer_id BIGINT,
  amount INT
);
