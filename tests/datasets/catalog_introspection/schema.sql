CREATE TABLE parent_accounts (
  id BIGINT,
  CONSTRAINT parent_accounts_pkey PRIMARY KEY (id)
);

CREATE TABLE accounts (
  id BIGINT,
  tenant_id BIGINT,
  email VARCHAR(255),
  parent_id BIGINT,
  balance BIGINT,
  CONSTRAINT accounts_pkey PRIMARY KEY (id),
  CONSTRAINT accounts_tenant_email_key UNIQUE (tenant_id, email),
  CONSTRAINT accounts_parent_fkey FOREIGN KEY (parent_id) REFERENCES parent_accounts(id),
  CONSTRAINT accounts_balance_check CHECK (balance >= 0)
);
