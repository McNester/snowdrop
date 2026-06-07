const path = require("path");
const express = require("express");
const bcrypt = require("bcryptjs");
const jwt = require("jsonwebtoken");
const { Pool } = require("pg");

const app = express();
const port = Number(process.env.DEMO_PORT || 8110);
const engine = process.env.ENGINE_LABEL || "PostgreSQL";
const databaseUrl = process.env.DATABASE_URL;
const jwtSecret = process.env.JWT_SECRET || "test-flight-demo-secret";
const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

if (!databaseUrl) {
  throw new Error("DATABASE_URL is required");
}

const pool = new Pool({
  connectionString: databaseUrl,
  max: Number(process.env.DB_POOL_SIZE || 8),
  idleTimeoutMillis: 10_000
});

app.use(express.json({ limit: "1mb" }));
app.use(express.static(path.join(__dirname, "public")));

function nowMs() {
  const [sec, nano] = process.hrtime();
  return sec * 1000 + nano / 1_000_000;
}

async function timedQuery(queryName, sql, values = []) {
  const runnableSql = values.length > 0 ? inlineSqlValues(sql, values) : sql;
  const started = nowMs();
  const result = await pool.query(runnableSql);
  return {
    result,
    metrics: {
      engine,
      queryName,
      dbLatencyMs: Number((nowMs() - started).toFixed(3)),
      rowCount: result.rowCount ?? result.rows.length,
      timestamp: new Date().toISOString()
    }
  };
}

function inlineSqlValues(sql, values) {
  return values.reduce((text, value, index) => {
    const placeholder = new RegExp(`\\$${index + 1}(?!\\d)`, "g");
    return text.replace(placeholder, sqlLiteral(value));
  }, sql);
}

function sqlLiteral(value) {
  if (value === null || value === undefined) {
    return "NULL";
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("Non-finite numeric SQL literal");
    }
    return String(value);
  }
  if (typeof value === "boolean") {
    return value ? "TRUE" : "FALSE";
  }
  if (value instanceof Date) {
    return `'${value.toISOString()}'`;
  }
  return `'${String(value).replace(/'/g, "''")}'`;
}

function allocateDemoId() {
  return Date.now() * 1000 + Math.floor(Math.random() * 1000);
}

function attachApiLatency(res, apiStarted, payload, metrics) {
  const apiLatencyMs = Number((nowMs() - apiStarted).toFixed(3));
  res.json({
    ...payload,
    _metrics: {
      ...metrics,
      apiLatencyMs
    }
  });
}

function requireAuth(req, res, next) {
  const header = req.headers.authorization || "";
  const token = header.startsWith("Bearer ") ? header.slice("Bearer ".length) : "";
  if (!token) {
    return res.status(401).json({ message: "Missing bearer token" });
  }
  try {
    req.user = jwt.verify(token, jwtSecret);
    return next();
  } catch {
    return res.status(401).json({ message: "Invalid bearer token" });
  }
}

function publicUser(row) {
  return {
    id: Number(row.id),
    name: row.full_name,
    email: row.email,
    role: row.role
  };
}

function flightRow(row) {
  return {
    id: Number(row.id),
    flightNumber: row.flight_number,
    fromCity: row.from_city,
    toCity: row.to_city,
    departAt: row.depart_at,
    arriveAt: row.arrive_at,
    price: Number(row.price),
    seatsAvailable: Number(row.seats_available),
    airlineName: row.airline_name,
    aircraftModel: row.aircraft_model
  };
}

function bookingRow(row) {
  return {
    id: Number(row.id),
    userId: Number(row.user_id),
    flightId: Number(row.flight_id),
    passengerName: row.passenger_name,
    passengerEmail: row.passenger_email,
    status: row.status,
    totalPrice: Number(row.total_price),
    createdAt: row.created_at,
    flight: {
      id: Number(row.flight_id),
      flightNumber: row.flight_number,
      fromCity: row.from_city,
      toCity: row.to_city,
      departAt: row.depart_at,
      arriveAt: row.arrive_at,
      price: Number(row.flight_price ?? row.price ?? row.total_price),
      seatsAvailable: Number(row.seats_available ?? 0)
    }
  };
}

function bookingFromRequest(bookingId, userId, flightId, passengerName, passengerEmail, flight) {
  const totalPrice = Number(flight.price || 0);
  return {
    id: bookingId,
    userId,
    flightId,
    passengerName,
    passengerEmail,
    status: "confirmed",
    totalPrice,
    createdAt: new Date().toISOString(),
    flight: {
      id: flightId,
      flightNumber: flight.flightNumber,
      fromCity: flight.fromCity,
      toCity: flight.toCity,
      departAt: flight.departAt || null,
      arriveAt: flight.arriveAt || null,
      price: totalPrice,
      seatsAvailable: Number(flight.seatsAvailable || 0)
    }
  };
}

app.get("/health", async (_req, res) => {
  const apiStarted = nowMs();
  try {
    const { metrics } = await timedQuery("health", "SELECT 1 AS ok");
    attachApiLatency(res, apiStarted, { status: "ok", engine }, metrics);
  } catch (error) {
    res.status(500).json({ status: "error", engine, message: error.message });
  }
});

app.get("/api/meta", (_req, res) => {
  res.json({
    engine,
    database: process.env.DATABASE_NAME || "test_flight_large",
    mode: process.env.DEMO_MODE || "single-engine"
  });
});

app.post("/api/auth/register", async (req, res) => {
  const apiStarted = nowMs();
  const { name, email, password } = req.body || {};
  if (!name || !email || !password) {
    return res.status(400).json({ message: "All fields are required" });
  }
  if (!emailPattern.test(email)) {
    return res.status(400).json({ message: "Valid email is required" });
  }
  try {
    const existing = await timedQuery(
      "register_check_email",
      "SELECT id FROM users WHERE email = $1",
      [email]
    );
    if (existing.result.rows.length > 0) {
      return res.status(409).json({ message: "Email already used", _metrics: existing.metrics });
    }

    const passwordHash = await bcrypt.hash(password, 8);
    const userId = allocateDemoId();
    const insert = await timedQuery(
      "register_user",
      "INSERT INTO users (id, email, full_name, password_hash, role, created_at) VALUES ($1, $2, $3, $4, 'user', $5)",
      [userId, email, name, passwordHash, new Date().toISOString()]
    );
    attachApiLatency(res, apiStarted, { user: { id: userId, name, email, role: "user" } }, insert.metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.post("/api/auth/login", async (req, res) => {
  const apiStarted = nowMs();
  const { email, password } = req.body || {};
  if (!email || !password) {
    return res.status(400).json({ message: "Email and password are required" });
  }
  try {
    const { result, metrics } = await timedQuery(
      "login_user_lookup",
      "SELECT id, email, full_name, password_hash, role FROM users WHERE email = $1",
      [email]
    );
    const user = result.rows[0];
    if (!user || !(await bcrypt.compare(password, user.password_hash))) {
      return res.status(401).json({ message: "Invalid credentials", _metrics: metrics });
    }
    const token = jwt.sign({ userId: Number(user.id), role: user.role }, jwtSecret, { expiresIn: "8h" });
    attachApiLatency(res, apiStarted, { token, user: publicUser(user) }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/auth/me", requireAuth, async (req, res) => {
  const apiStarted = nowMs();
  try {
    const { result, metrics } = await timedQuery(
      "profile_lookup",
      "SELECT id, email, full_name, role FROM users WHERE id = $1",
      [req.user.userId]
    );
    if (result.rows.length === 0) {
      return res.status(404).json({ message: "User not found", _metrics: metrics });
    }
    attachApiLatency(res, apiStarted, { user: publicUser(result.rows[0]) }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/flights", async (req, res) => {
  const apiStarted = nowMs();
  const from = String(req.query.from || "").trim();
  const to = String(req.query.to || "").trim();
  const date = String(req.query.date || "").trim();
  const passengers = Number(req.query.passengers || 1);
  if (from && to && from === to) {
    return res.status(400).json({ message: "From and To must be different" });
  }
  if (!Number.isInteger(passengers) || passengers < 1 || passengers > 9) {
    return res.status(400).json({ message: "Passengers must be between 1 and 9" });
  }
  const filters = [];
  const values = [];
  if (from) {
    values.push(from);
    filters.push(`from_city = $${values.length}`);
  }
  if (to) {
    values.push(to);
    filters.push(`to_city = $${values.length}`);
  }
  if (date) {
    values.push(date);
    filters.push(`depart_day = $${values.length}`);
  }
  const whereSql = filters.length > 0 ? `WHERE ${filters.join(" AND ")}` : "";
  const sql = `
    SELECT id, flight_number, from_city, to_city, depart_at, arrive_at,
           price, seats_available, airline_name, aircraft_model
    FROM flights
    ${whereSql}
    ORDER BY price ASC, depart_at ASC
    LIMIT 80`;
  try {
    const { result, metrics } = await timedQuery("search_flights", sql, values);
    attachApiLatency(res, apiStarted, { flights: result.rows.map(flightRow) }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/flights/:id", async (req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT id, flight_number, from_city, to_city, depart_at, arrive_at,
           price, seats_available, airline_name, aircraft_model
    FROM flights
    WHERE id = $1`;
  try {
    const { result, metrics } = await timedQuery("flight_details", sql, [Number(req.params.id)]);
    if (result.rows.length === 0) {
      return res.status(404).json({ message: "Flight not found", _metrics: metrics });
    }
    attachApiLatency(res, apiStarted, { flight: flightRow(result.rows[0]) }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.post("/api/bookings", requireAuth, async (req, res) => {
  const apiStarted = nowMs();
  const {
    flightId,
    passengerName,
    passengerEmail,
    flightNumber,
    fromCity,
    toCity,
    price,
    departAt,
    arriveAt,
    seatsAvailable
  } = req.body || {};
  if (!flightId || !passengerName || !passengerEmail) {
    return res.status(400).json({ message: "Missing required fields" });
  }
  if (!emailPattern.test(passengerEmail)) {
    return res.status(400).json({ message: "Valid passenger email is required" });
  }
  try {
    let flight = {
      flightNumber,
      fromCity,
      toCity,
      price,
      departAt,
      arriveAt,
      seatsAvailable
    };
    if (!flight.flightNumber || !flight.fromCity || !flight.toCity || flight.price === undefined) {
      const lookup = await timedQuery(
        "booking_flight_lookup",
        "SELECT id, flight_number, from_city, to_city, depart_at, arrive_at, price, seats_available FROM flights WHERE id = $1",
        [Number(flightId)]
      );
      if (lookup.result.rows.length === 0) {
        return res.status(404).json({ message: "Flight not found", _metrics: lookup.metrics });
      }
      flight = {
        flightNumber: lookup.result.rows[0].flight_number,
        fromCity: lookup.result.rows[0].from_city,
        toCity: lookup.result.rows[0].to_city,
        price: lookup.result.rows[0].price,
        departAt: lookup.result.rows[0].depart_at,
        arriveAt: lookup.result.rows[0].arrive_at,
        seatsAvailable: lookup.result.rows[0].seats_available
      };
    }
    const bookingId = allocateDemoId();
    const insert = await timedQuery(
      "book_flight",
      "INSERT INTO bookings (id, user_id, flight_id, passenger_name, passenger_email, status, total_price, flight_number, from_city, to_city, created_at) VALUES ($1, $2, $3, $4, $5, 'confirmed', $6, $7, $8, $9, $10)",
      [
        bookingId,
        req.user.userId,
        Number(flightId),
        passengerName,
        passengerEmail,
        Number(flight.price),
        flight.flightNumber,
        flight.fromCity,
        flight.toCity,
        new Date().toISOString()
      ]
    );
    attachApiLatency(
      res,
      apiStarted,
      { booking: bookingFromRequest(bookingId, req.user.userId, Number(flightId), passengerName, passengerEmail, flight) },
      insert.metrics
    );
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/bookings/my", requireAuth, async (req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT id, user_id, flight_id, passenger_name, passenger_email, status, total_price, created_at,
           flight_number, from_city, to_city
    FROM bookings
    WHERE user_id = $1
    ORDER BY created_at DESC
    LIMIT 50`;
  try {
    const { result, metrics } = await timedQuery("my_bookings", sql, [req.user.userId]);
    attachApiLatency(res, apiStarted, { bookings: result.rows.map(bookingRow) }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.patch("/api/bookings/:id/cancel", requireAuth, async (req, res) => {
  const apiStarted = nowMs();
  try {
    const updated = await timedQuery(
      "cancel_booking",
      "UPDATE bookings SET status = 'cancelled' WHERE id = $1",
      [Number(req.params.id)]
    );
    if (updated.result.rowCount === 0) {
      return res.status(404).json({ message: "Booking not found", _metrics: updated.metrics });
    }
    attachApiLatency(
      res,
      apiStarted,
      { booking: { id: Number(req.params.id), status: "cancelled" } },
      updated.metrics
    );
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/dashboard/routes", async (_req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT from_city, to_city, SUM(search_count) AS search_count, SUM(booking_count) AS booking_count,
           SUM(revenue) AS revenue
    FROM route_daily_stats
    GROUP BY from_city, to_city
    ORDER BY search_count DESC, revenue DESC
    LIMIT 20`;
  try {
    const { result, metrics } = await timedQuery("route_dashboard", sql);
    attachApiLatency(res, apiStarted, { routes: result.rows }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/dashboard/activity", async (_req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT event_day, event_kind, COUNT(*) AS activity_count
    FROM flight_search_events
    GROUP BY event_day, event_kind
    ORDER BY event_day DESC, event_kind
    LIMIT 40`;
  try {
    const { result, metrics } = await timedQuery("activity_rollup", sql);
    attachApiLatency(res, apiStarted, { activity: result.rows }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/dashboard/demand", async (_req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT from_city, SUM(search_count) AS search_count, SUM(booking_count) AS booking_count,
           SUM(revenue) AS revenue
    FROM route_daily_stats
    GROUP BY from_city
    ORDER BY search_count DESC, revenue DESC
    LIMIT 20`;
  try {
    const { result, metrics } = await timedQuery("route_origin_demand", sql);
    attachApiLatency(res, apiStarted, { demand: result.rows }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/api/dashboard/flight-status", async (_req, res) => {
  const apiStarted = nowMs();
  const sql = `
    SELECT flight_status, COUNT(*) AS flights, SUM(seats_available) AS seats
    FROM flights
    WHERE from_city = 'Kyiv' AND to_city = 'Warsaw'
    GROUP BY flight_status
    ORDER BY flight_status`;
  try {
    const { result, metrics } = await timedQuery("flight_status_rollup", sql);
    attachApiLatency(res, apiStarted, { statuses: result.rows }, metrics);
  } catch (error) {
    res.status(500).json({ message: error.message, engine });
  }
});

app.get("/{*path}", (_req, res) => {
  res.sendFile(path.join(__dirname, "public", "index.html"));
});

app.listen(port, "0.0.0.0", () => {
  console.log(`Test Flight ${engine} demo listening on http://0.0.0.0:${port}`);
});
