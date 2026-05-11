-- Snowdrop migration translation preview
-- Source PostgreSQL database: catalog_introspection
-- Source schema filter: public

-- Source table public.accounts
CREATE TABLE "accounts" (
  "id" BIGINT NOT NULL,
  "tenant_id" BIGINT,
  "email" VARCHAR(255),
  "parent_id" BIGINT,
  "balance" BIGINT,
  FOREIGN KEY ("parent_id") REFERENCES "parent_accounts" ("id"),
  PRIMARY KEY ("id"),
  UNIQUE ("tenant_id", "email")
);

-- NOTE: check constraint 'accounts_balance_check' is omitted from the preview and should be re-added manually after verification
-- NOTE: index 'accounts_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'accounts_tenant_email_key' is omitted from the preview and should be recreated manually if still needed

-- Source table public.parent_accounts
CREATE TABLE "parent_accounts" (
  "id" BIGINT NOT NULL,
  PRIMARY KEY ("id")
);

-- NOTE: index 'parent_accounts_pkey' is omitted from the preview and should be recreated manually if still needed

