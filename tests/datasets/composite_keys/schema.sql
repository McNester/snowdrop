CREATE TABLE memberships (
  tenant_id BIGINT,
  user_id BIGINT,
  role VARCHAR(16),
  PRIMARY KEY (tenant_id, user_id)
);
