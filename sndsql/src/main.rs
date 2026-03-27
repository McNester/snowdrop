use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::process;
use tokio_postgres::{NoTls, SimpleQueryMessage};

#[derive(Parser)]
#[command(name = "sndsql", about = "Snowdrop interactive SQL client")]
struct Args {
    /// Database host
    #[arg(short = 'H', long, default_value = "localhost")]
    host: String,

    /// Port number
    #[arg(short, long, default_value_t = 5432)]
    port: u16,

    /// Username
    #[arg(short = 'U', long, default_value = "snowdrop")]
    user: String,

    /// Database name
    #[arg(short, long, default_value = "snowdrop")]
    dbname: String,

    /// Execute a single command and exit
    #[arg(short, long)]
    command: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let conn_str = format!(
        "host={} port={} user={} dbname={}",
        args.host, args.port, args.user, args.dbname
    );

    let (client, connection) = match tokio_postgres::connect(&conn_str, NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("sndsql: connection failed: {}", e);
            process::exit(1);
        }
    };

    // Drive the connection in the background
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("sndsql: connection error: {}", e);
        }
    });

    if let Some(cmd) = args.command {
        execute_query(&client, &cmd).await;
        return;
    }

    // Interactive REPL
    println!(
        "sndsql (Snowdrop SQL client) connected to {}:{}\nType \\q to quit.\n",
        args.host, args.port
    );

    let mut rl = DefaultEditor::new().expect("failed to init editor");
    let mut buffer = String::new();

    loop {
        let prompt = if buffer.is_empty() { "snowdrop> " } else { "       -> " };

        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();

                if trimmed == "\\q" || trimmed == "quit" || trimmed == "exit" {
                    break;
                }

                if trimmed.is_empty() {
                    continue;
                }

                buffer.push_str(trimmed);

                // Execute when we see a semicolon
                if buffer.ends_with(';') {
                    let _ = rl.add_history_entry(&buffer);
                    execute_query(&client, &buffer).await;
                    buffer.clear();
                } else {
                    buffer.push(' ');
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("sndsql: input error: {}", e);
                break;
            }
        }
    }
}

async fn execute_query(client: &tokio_postgres::Client, query: &str) {
    let start = std::time::Instant::now();

    match client.simple_query(query).await {
        Ok(messages) => {
            let mut rows: Vec<Vec<String>> = Vec::new();
            let mut columns: Vec<String> = Vec::new();
            let mut tag: Option<String> = None;

            for msg in &messages {
                match msg {
                    SimpleQueryMessage::Row(row) => {
                        if columns.is_empty() {
                            columns = row
                                .columns()
                                .iter()
                                .map(|c| c.name().to_string())
                                .collect();
                        }
                        let vals: Vec<String> = (0..row.len())
                            .map(|i| {
                                row.get(i)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| "NULL".to_string())
                            })
                            .collect();
                        rows.push(vals);
                    }
                    SimpleQueryMessage::CommandComplete(n) => {
                        tag = Some(format!("{}", n));
                    }
                    _ => {}
                }
            }

            if !columns.is_empty() {
                print_table(&columns, &rows);
                println!("({} rows)\n", rows.len());
            } else if let Some(t) = tag {
                println!("{}\n", t);
            }

            let elapsed = start.elapsed();
            println!("Time: {:.3}ms", elapsed.as_secs_f64() * 1000.0);
        }
        Err(e) => {
            if let Some(db_err) = e.as_db_error() {
                eprintln!("ERROR: {}\n", db_err.message());
            } else {
                eprintln!("ERROR: {}\n", e);
            }
        }
    }
}

/// Print results in a formatted ASCII table.
fn print_table(columns: &[String], rows: &[Vec<String>]) {
    if columns.is_empty() {
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
    for row in rows {
        for (i, val) in row.iter().enumerate() {
            if i < widths.len() && val.len() > widths[i] {
                widths[i] = val.len();
            }
        }
    }

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!(" {:width$} ", c, width = widths[i]))
        .collect();
    println!("{}", header.join("|"));

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(w + 2)).collect();
    println!("{}", sep.join("+"));

    // Rows
    for row in rows {
        let formatted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let w = if i < widths.len() { widths[i] } else { v.len() };
                format!(" {:width$} ", v, width = w)
            })
            .collect();
        println!("{}", formatted.join("|"));
    }
}
