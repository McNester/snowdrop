CREATE TABLE items (
  id BIGINT PRIMARY KEY,
  name VARCHAR(255),
  category VARCHAR(255),
  price FLOAT
);

CREATE TABLE sales (
  id BIGINT PRIMARY KEY,
  item_id BIGINT,
  qty INT
);
