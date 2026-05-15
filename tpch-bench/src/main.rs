use tpch_bench::{parse_args, run_benchmark_suite, write_json_output};


#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let options = parse_args(&args);

    match run_benchmark_suite(&options).await {
        Ok(results) => {
            if let Some(path) = options.output_path.as_deref() {
                if let Err(err) = write_json_output(&results, path) {
                    eprintln!("Could not write JSON output: {}", err);
                }
            }
        }
        Err(err) => {
            eprintln!("Benchmark failed: {}", err);
            std::process::exit(1);
        }
    }
}
