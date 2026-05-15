CREATE TABLE blocked_orders (
  id BIGSERIAL PRIMARY KEY,
  payload JSONB NOT NULL,
  tags TEXT[] NOT NULL DEFAULT ARRAY['new'],
  created_at TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX idx_blocked_orders_payload ON blocked_orders USING GIN (payload);
