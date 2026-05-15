CREATE TABLE parents (
  id BIGINT PRIMARY KEY,
  name VARCHAR(32)
);

CREATE TABLE children (
  id BIGINT PRIMARY KEY,
  parent_id BIGINT,
  label VARCHAR(32),
  FOREIGN KEY (parent_id) REFERENCES parents(id)
);
