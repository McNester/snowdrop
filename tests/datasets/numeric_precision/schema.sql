CREATE TABLE ledger (
  id BIGINT PRIMARY KEY,
  category VARCHAR(32),
  amount DECIMAL(10,2),
  fee NUMERIC(10,3)
);
