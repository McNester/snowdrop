use rand::Rng;
use std::time::Instant;
use tokio_postgres::{NoTls, SimpleQueryMessage};

const SCALE: usize = 1; // SF0.01 equivalent: ~600 lineitem, ~150 orders, ~15 customers

// Multipliers for scale factor
const CUSTOMERS: usize = 150 * SCALE;
const ORDERS: usize = 1500 * SCALE;
const LINEITEMS_PER_ORDER: usize = 4; // ~6000 lineitem rows at SCALE=1
const PARTS: usize = 200 * SCALE;

// TPC-H constants
const SEGMENTS: [&str; 5] = ["AUTOMOBILE", "BUILDING", "FURNITURE", "MACHINERY", "HOUSEHOLD"];
const RETURN_FLAGS: [&str; 3] = ["A", "N", "R"];
const LINE_STATUSES: [&str; 2] = ["F", "O"];
const PART_TYPES: [&str; 6] = [
    "PROMO BURNISHED COPPER", "STANDARD POLISHED TIN",
    "PROMO ANODIZED STEEL", "ECONOMY BRUSHED BRASS",
    "SMALL PLATED NICKEL", "MEDIUM BURNISHED IRON",
];

struct Config {
    host: String,
    port: u16,
    user: String,
    dbname: String,
    label: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scale = if args.len() > 1 { args[1].parse::<usize>().unwrap_or(1) } else { 1 };

    println!("TPC-H Benchmark (scale={}, ~{} lineitem rows)", scale, scale * ORDERS * LINEITEMS_PER_ORDER);

    // Snowdrop config
    let snowdrop = Config {
        host: "localhost".into(),
        port: 5432,
        user: "snowdrop".into(),
        dbname: "snowdrop".into(),
        label: "Snowdrop".into(),
    };

    // PostgreSQL config
    let postgres = Config {
        host: "localhost".into(),
        port: 5433,
        user: "postgres".into(),
        dbname: "tpch".into(),
        label: "PostgreSQL 16".into(),
    };

    println!("\n--- Loading data into {} ---", snowdrop.label);
    if let Err(e) = load_and_benchmark(&snowdrop, scale, true).await {
        eprintln!("Snowdrop error: {}", e);
    }

    println!("\n--- Loading data into {} ---", postgres.label);
    if let Err(e) = load_and_benchmark(&postgres, scale, false).await {
        eprintln!("PostgreSQL error: {}", e);
    }
}

async fn load_and_benchmark(
    config: &Config,
    scale: usize,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let password = if is_snowdrop { "" } else { " password=postgres" };
    let conn_str = format!(
        "host={} port={} user={} dbname={}{}",
        config.host, config.port, config.user, config.dbname, password
    );
    let (client, connection) = tokio_postgres::connect(&conn_str, NoTls).await?;
    tokio::spawn(async move { let _ = connection.await; });

    // Create schema
    create_schema(&client, is_snowdrop).await?;

    // Generate and load data
    let total_lineitems = scale * ORDERS * LINEITEMS_PER_ORDER;
    println!("Loading {} customers, {} orders, {} lineitem, {} part...",
        scale * CUSTOMERS, scale * ORDERS, total_lineitems, scale * PARTS);
    let load_start = Instant::now();
    load_data(&client, scale, is_snowdrop).await?;
    println!("Load time: {:.1}s", load_start.elapsed().as_secs_f64());

    // Run benchmark queries
    println!("\n=== Benchmark: {} ===\n", config.label);
    run_benchmark(&client, is_snowdrop).await?;

    Ok(())
}

async fn create_schema(
    client: &tokio_postgres::Client,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_snowdrop {
        let _ = client.simple_query("CREATE DATABASE tpch;").await;
        let _ = client.simple_query("DROP TABLE tpch.lineitem;").await;
        let _ = client.simple_query("DROP TABLE tpch.orders;").await;
        let _ = client.simple_query("DROP TABLE tpch.customer;").await;
        let _ = client.simple_query("DROP TABLE tpch.part;").await;

        client.simple_query(
            "CREATE TABLE tpch.customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE tpch.part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);"
        ).await?;
    } else {
        let _ = client.simple_query("DROP TABLE IF EXISTS lineitem, orders, customer, part CASCADE;").await;

        client.simple_query(
            "CREATE TABLE customer (c_custkey BIGINT PRIMARY KEY, c_name VARCHAR(255), c_mktsegment VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE orders (o_orderkey BIGINT PRIMARY KEY, o_custkey BIGINT, o_orderstatus VARCHAR(1), o_totalprice FLOAT, o_orderdate VARCHAR(255), o_shippriority INT);"
        ).await?;
        client.simple_query(
            "CREATE TABLE lineitem (l_id BIGINT PRIMARY KEY, l_orderkey BIGINT, l_partkey BIGINT, l_quantity FLOAT, l_extendedprice FLOAT, l_discount FLOAT, l_tax FLOAT, l_returnflag VARCHAR(1), l_linestatus VARCHAR(1), l_shipdate VARCHAR(255));"
        ).await?;
        client.simple_query(
            "CREATE TABLE part (p_partkey BIGINT PRIMARY KEY, p_name VARCHAR(255), p_type VARCHAR(255), p_retailprice FLOAT);"
        ).await?;
    }
    Ok(())
}

async fn load_data(
    client: &tokio_postgres::Client,
    scale: usize,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();
    let prefix = if is_snowdrop { "tpch." } else { "" };
    let batch_size = 100;

    // Load customers
    let num_customers = scale * CUSTOMERS;
    for batch_start in (1..=num_customers).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_customers);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let seg = SEGMENTS[i % SEGMENTS.len()];
                format!("({}, 'Customer#{:06}', '{}')", i, i, seg)
            })
            .collect();
        let sql = format!("INSERT INTO {}customer (c_custkey, c_name, c_mktsegment) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    // Load parts
    let num_parts = scale * PARTS;
    for batch_start in (1..=num_parts).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_parts);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let ptype = PART_TYPES[i % PART_TYPES.len()];
                let price = 900.0 + (i as f64 * 0.01);
                format!("({}, 'Part#{:06}', '{}', {:.2})", i, i, ptype, price)
            })
            .collect();
        let sql = format!("INSERT INTO {}part (p_partkey, p_name, p_type, p_retailprice) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    // Load orders
    let num_orders = scale * ORDERS;
    for batch_start in (1..=num_orders).step_by(batch_size) {
        let batch_end = (batch_start + batch_size - 1).min(num_orders);
        let vals: Vec<String> = (batch_start..=batch_end)
            .map(|i| {
                let custkey = (i % num_customers) + 1;
                let status = if i % 2 == 0 { "O" } else { "F" };
                let total = 1000.0 + (i as f64 * 1.5);
                let year = 1992 + (i % 7);
                let month = (i % 12) + 1;
                let day = (i % 28) + 1;
                let date = format!("{:04}-{:02}-{:02}", year, month, day);
                let priority = i % 5;
                format!("({}, {}, '{}', {:.2}, '{}', {})", i, custkey, status, total, date, priority)
            })
            .collect();
        let sql = format!("INSERT INTO {}orders (o_orderkey, o_custkey, o_orderstatus, o_totalprice, o_orderdate, o_shippriority) VALUES {};", prefix, vals.join(", "));
        client.simple_query(&sql).await?;
    }

    // Load lineitem
    let mut li_id = 1u64;
    for order_id in 1..=num_orders {
        let num_items: usize = rng.gen_range(1..=LINEITEMS_PER_ORDER * 2);
        let mut batch_vals: Vec<String> = Vec::new();

        for _ in 0..num_items {
            let partkey = rng.gen_range(1..=num_parts);
            let qty: f64 = rng.gen_range(1.0..50.0);
            let price: f64 = rng.gen_range(900.0..100000.0);
            let discount: f64 = (rng.gen_range(0u32..11) as f64) / 100.0;
            let tax: f64 = (rng.gen_range(0u32..9) as f64) / 100.0;
            let rflag = RETURN_FLAGS[rng.gen_range(0..3)];
            let lstatus = LINE_STATUSES[rng.gen_range(0..2)];
            let year = 1992 + rng.gen_range(0..7);
            let month = rng.gen_range(1..=12);
            let day = rng.gen_range(1..=28);
            let shipdate = format!("{:04}-{:02}-{:02}", year, month, day);

            batch_vals.push(format!(
                "({}, {}, {}, {:.2}, {:.2}, {:.2}, {:.2}, '{}', '{}', '{}')",
                li_id, order_id, partkey, qty, price, discount, tax, rflag, lstatus, shipdate
            ));
            li_id += 1;

            if batch_vals.len() >= batch_size {
                let sql = format!(
                    "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
                    prefix, batch_vals.join(", ")
                );
                client.simple_query(&sql).await?;
                batch_vals.clear();
            }
        }

        if !batch_vals.is_empty() {
            let sql = format!(
                "INSERT INTO {}lineitem (l_id, l_orderkey, l_partkey, l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate) VALUES {};",
                prefix, batch_vals.join(", ")
            );
            client.simple_query(&sql).await?;
        }
    }

    println!("Loaded {} lineitem rows", li_id - 1);
    Ok(())
}

async fn run_benchmark(
    client: &tokio_postgres::Client,
    is_snowdrop: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = if is_snowdrop { "tpch." } else { "" };

    // Q6: Revenue from discounted items in date range
    let q6 = format!(
        "SELECT SUM(l_extendedprice * l_discount) AS revenue \
         FROM {}lineitem \
         WHERE l_shipdate >= '1994-01-01' AND l_shipdate < '1995-01-01' \
         AND l_discount >= 0.05 AND l_discount <= 0.07 \
         AND l_quantity < 24;",
        prefix
    );

    // Q1: Pricing summary report
    let q1 = format!(
        "SELECT l_returnflag, l_linestatus, \
         SUM(l_quantity) AS sum_qty, \
         SUM(l_extendedprice) AS sum_base_price, \
         SUM(l_extendedprice * (1 - l_discount)) AS sum_disc_price, \
         AVG(l_quantity) AS avg_qty, \
         AVG(l_extendedprice) AS avg_price, \
         AVG(l_discount) AS avg_disc, \
         COUNT(*) AS count_order \
         FROM {}lineitem \
         WHERE l_shipdate <= '1998-09-02' \
         GROUP BY l_returnflag, l_linestatus \
         ORDER BY l_returnflag, l_linestatus;",
        prefix
    );

    // Q14: Promotion effect (simplified, no CASE for Snowdrop)
    let q14 = format!(
        "SELECT SUM(CASE WHEN p_type LIKE 'PROMO%' THEN l_extendedprice * (1 - l_discount) ELSE 0 END) AS promo_revenue, \
         SUM(l_extendedprice * (1 - l_discount)) AS total_revenue \
         FROM {}lineitem l INNER JOIN {}part p ON l.l_partkey = p.p_partkey \
         WHERE l.l_shipdate >= '1995-01-01' AND l.l_shipdate < '1995-02-01';",
        prefix, prefix
    );

    // Q3: Shipping priority (3-table join)
    let q3 = format!(
        "SELECT l.l_orderkey, SUM(l.l_extendedprice * (1 - l.l_discount)) AS revenue, o.o_orderdate, o.o_shippriority \
         FROM {}customer c \
         INNER JOIN {}orders o ON c.c_custkey = o.o_custkey \
         INNER JOIN {}lineitem l ON l.l_orderkey = o.o_orderkey \
         WHERE c.c_mktsegment = 'BUILDING' \
         AND o.o_orderdate < '1995-03-15' \
         AND l.l_shipdate > '1995-03-15' \
         GROUP BY l.l_orderkey, o.o_orderdate, o.o_shippriority \
         ORDER BY revenue DESC, o.o_orderdate \
         LIMIT 10;",
        prefix, prefix, prefix
    );

    let queries = [
        ("Q6 (SUM with date range)", &q6),
        ("Q1 (GROUP BY + aggregates)", &q1),
        ("Q14 (JOIN + CASE WHEN)", &q14),
        ("Q3 (3-table JOIN + GROUP BY)", &q3),
    ];

    for (name, sql) in &queries {
        println!("--- {} ---", name);
        // Run 3 times, report each
        for run in 1..=3 {
            let start = Instant::now();
            match client.simple_query(sql).await {
                Ok(msgs) => {
                    let elapsed = start.elapsed();
                    let row_count = msgs.iter().filter(|m| matches!(m, SimpleQueryMessage::Row(_))).count();
                    if run == 1 {
                        // Print first run results
                        for msg in &msgs {
                            if let SimpleQueryMessage::Row(row) = msg {
                                let vals: Vec<String> = (0..row.len())
                                    .map(|i| row.get(i).unwrap_or("NULL").to_string())
                                    .collect();
                                println!("  {}", vals.join(" | "));
                            }
                        }
                    }
                    println!("  Run {}: {:.1}ms ({} rows)", run, elapsed.as_secs_f64() * 1000.0, row_count);
                }
                Err(e) => {
                    eprintln!("  ERROR: {}", e);
                    break;
                }
            }
        }
        println!();
    }

    Ok(())
}
