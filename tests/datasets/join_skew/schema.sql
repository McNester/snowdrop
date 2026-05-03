CREATE TABLE customers (
  id BIGINT PRIMARY KEY,
  name VARCHAR(255),
  tier VARCHAR(255)
);

CREATE TABLE orders (
  id BIGINT PRIMARY KEY,
  customer_id BIGINT,
  total FLOAT
);
