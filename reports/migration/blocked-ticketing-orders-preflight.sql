-- Snowdrop migration translation preview
-- Source PostgreSQL database: blocked_ticketing_orders

-- Source table public.blocked_orders
CREATE TABLE "blocked_orders" (
  "id" BIGINT NOT NULL,
  "created_at" TIMESTAMP NOT NULL,
  PRIMARY KEY ("id")
);

-- NOTE: default for column 'id' needs manual rewrite and is omitted from preview
-- NOTE: column 'payload' has no safe Snowdrop type mapping yet
-- NOTE: column 'tags' has no safe Snowdrop type mapping yet
-- NOTE: default for column 'created_at' needs manual rewrite and is omitted from preview
-- NOTE: index 'blocked_orders_pkey' is omitted from the preview and should be recreated manually if still needed
-- NOTE: index 'idx_blocked_orders_payload' is omitted from the preview and should be recreated manually if still needed

