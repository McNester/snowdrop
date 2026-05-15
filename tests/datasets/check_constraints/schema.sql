CREATE TABLE inventory (
  id BIGINT PRIMARY KEY,
  qty INT,
  price INT,
  CHECK (qty >= 0),
  CHECK (price > 0)
);
