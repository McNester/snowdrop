import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const postgresBaseUrl = process.env.POSTGRES_BASE_URL || "http://localhost:8110";
const snowdropBaseUrl = process.env.SNOWDROP_BASE_URL || "http://localhost:8120";
const runs = Number(process.env.RUNS || 5);
const outputDir = process.env.OUTPUT_DIR || path.resolve(repoRoot(), "reports", "test-flight-demo");
const demoEmail = process.env.DEMO_EMAIL || "demo@testflight.local";
const demoPassword = process.env.DEMO_PASSWORD || "demo12345";

function repoRoot() {
  const current = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(current, "..", "..", "..");
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length === 0) return 0;
  if (sorted.length % 2 === 1) return sorted[middle];
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

function average(values) {
  if (values.length === 0) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function summarize(values) {
  return {
    median: Number(median(values).toFixed(3)),
    average: Number(average(values).toFixed(3)),
    min: Number(Math.min(...values).toFixed(3)),
    max: Number(Math.max(...values).toFixed(3))
  };
}

async function request(baseUrl, endpoint, options = {}) {
  const started = performance.now();
  const response = await fetch(`${baseUrl}${endpoint}`, options);
  const body = await response.json();
  const clientLatencyMs = Number((performance.now() - started).toFixed(3));
  if (!response.ok) {
    throw new Error(`${baseUrl}${endpoint} failed: ${body.message || response.status}`);
  }
  return { body, clientLatencyMs };
}

function authHeaders(token) {
  return {
    "Content-Type": "application/json",
    Authorization: `Bearer ${token}`
  };
}

function publicHeaders() {
  return { "Content-Type": "application/json" };
}

function pickMetrics(action, response) {
  const metrics = response.body._metrics || {};
  return {
    action,
    queryName: metrics.queryName || action,
    dbLatencyMs: Number(metrics.dbLatencyMs || 0),
    apiLatencyMs: Number(metrics.apiLatencyMs || response.clientLatencyMs),
    clientLatencyMs: response.clientLatencyMs,
    rowCount: Number(metrics.rowCount || 0)
  };
}

async function runScenario(baseUrl, label, iteration) {
  const actions = [];

  const health = await request(baseUrl, "/health");
  actions.push(pickMetrics("health", health));

  const login = await request(baseUrl, "/api/auth/login", {
    method: "POST",
    headers: publicHeaders(),
    body: JSON.stringify({ email: demoEmail, password: demoPassword })
  });
  actions.push(pickMetrics("login", login));
  const token = login.body.token;

  const searchParams = new URLSearchParams({
    from: "Kyiv",
    to: "Warsaw",
    date: "2026-06-22",
    passengers: "1"
  });
  const search = await request(baseUrl, `/api/flights?${searchParams.toString()}`);
  actions.push(pickMetrics("search_flights_button", search));
  const firstFlight = search.body.flights?.[iteration % search.body.flights.length];
  if (!firstFlight) {
    throw new Error(`${label} returned no flights for the demo search`);
  }

  const passengerEmail = `action-${label.toLowerCase()}-${Date.now()}-${iteration}@testflight.local`;
  const booking = await request(baseUrl, "/api/bookings", {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify({
      flightId: firstFlight.id,
      passengerName: "Action Bench Passenger",
      passengerEmail,
      flightNumber: firstFlight.flightNumber,
      fromCity: firstFlight.fromCity,
      toCity: firstFlight.toCity,
      price: firstFlight.price,
      departAt: firstFlight.departAt,
      arriveAt: firstFlight.arriveAt,
      seatsAvailable: firstFlight.seatsAvailable
    })
  });
  actions.push(pickMetrics("book_flight_button", booking));
  const bookingId = booking.body.booking.id;

  const myBookings = await request(baseUrl, "/api/bookings/my", {
    headers: authHeaders(token)
  });
  actions.push(pickMetrics("my_bookings_button", myBookings));

  const cancel = await request(baseUrl, `/api/bookings/${bookingId}/cancel`, {
    method: "PATCH",
    headers: authHeaders(token)
  });
  actions.push(pickMetrics("cancel_booking_button", cancel));

  const routes = await request(baseUrl, "/api/dashboard/routes");
  actions.push(pickMetrics("dashboard_routes_button", routes));

  const flightStatus = await request(baseUrl, "/api/dashboard/flight-status");
  actions.push(pickMetrics("dashboard_flight_status_button", flightStatus));

  return actions;
}

function groupByAction(results) {
  const grouped = new Map();
  for (const result of results) {
    const current = grouped.get(result.action) || [];
    current.push(result);
    grouped.set(result.action, current);
  }
  return grouped;
}

function compare(postgresResults, snowdropResults) {
  const pgGrouped = groupByAction(postgresResults);
  const sdGrouped = groupByAction(snowdropResults);
  const actions = [...new Set([...pgGrouped.keys(), ...sdGrouped.keys()])];
  return actions.map((action) => {
    const pgRows = pgGrouped.get(action) || [];
    const sdRows = sdGrouped.get(action) || [];
    const pgDb = summarize(pgRows.map((row) => row.dbLatencyMs));
    const sdDb = summarize(sdRows.map((row) => row.dbLatencyMs));
    const pgApi = summarize(pgRows.map((row) => row.apiLatencyMs));
    const sdApi = summarize(sdRows.map((row) => row.apiLatencyMs));
    const ratio = sdDb.median > 0 ? pgDb.median / sdDb.median : 0;
    const direction = ratio >= 1
      ? `Snowdrop is ${ratio.toFixed(2)}x faster`
      : `Snowdrop is ${(1 / ratio).toFixed(2)}x slower`;
    return {
      action,
      postgresDb: pgDb,
      snowdropDb: sdDb,
      postgresApi: pgApi,
      snowdropApi: sdApi,
      ratio: Number(ratio.toFixed(3)),
      direction
    };
  });
}

function markdownReport(summary) {
  const rows = summary.comparison.map((row) => (
    `| ${row.action} | ${row.postgresDb.median} | ${row.snowdropDb.median} | ${row.postgresApi.median} | ${row.snowdropApi.median} | ${row.direction} |`
  ));
  return [
    "# Test Flight Action Benchmark",
    "",
    `Generated: ${summary.generatedAt}`,
    "",
    `PostgreSQL site: ${summary.postgresBaseUrl}`,
    "",
    `Snowdrop site: ${summary.snowdropBaseUrl}`,
    "",
    `Runs: ${summary.runs}`,
    "",
    "| Action | PostgreSQL DB median ms | Snowdrop DB median ms | PostgreSQL API median ms | Snowdrop API median ms | Result |",
    "| --- | ---: | ---: | ---: | ---: | --- |",
    ...rows,
    "",
    "Notes:",
    "",
    "- This test drives the same HTTP actions as the local Test Flight UI buttons.",
    "- DB latency is reported by the backend around the SQL call.",
    "- API latency is end-to-end server handler latency, excluding browser rendering.",
    "- The goal is not to hide slower write-path actions; the report marks them explicitly."
  ].join("\n");
}

const postgresResults = [];
const snowdropResults = [];

for (let iteration = 0; iteration < runs; iteration += 1) {
  postgresResults.push(...await runScenario(postgresBaseUrl, "PostgreSQL", iteration));
  snowdropResults.push(...await runScenario(snowdropBaseUrl, "Snowdrop", iteration));
}

const summary = {
  schemaVersion: 1,
  generatedAt: new Date().toISOString(),
  runs,
  postgresBaseUrl,
  snowdropBaseUrl,
  comparison: compare(postgresResults, snowdropResults),
  raw: {
    postgres: postgresResults,
    snowdrop: snowdropResults
  }
};

await mkdir(outputDir, { recursive: true });
await writeFile(path.join(outputDir, "action-comparison.json"), `${JSON.stringify(summary, null, 2)}\n`);
await writeFile(path.join(outputDir, "action-comparison.md"), `${markdownReport(summary)}\n`);

console.log(markdownReport(summary));
console.log(`\nWrote ${path.join(outputDir, "action-comparison.json")}`);
console.log(`Wrote ${path.join(outputDir, "action-comparison.md")}`);
