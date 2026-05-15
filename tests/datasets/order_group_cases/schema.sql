CREATE TABLE categories (
  id BIGINT PRIMARY KEY,
  name VARCHAR(64)
);

CREATE TABLE sales (
  id BIGINT PRIMARY KEY,
  category_id BIGINT,
  qty INT
);
