use tpch_bench::{
    build_json_report, parse_args, run_benchmark_suite, write_json_report, write_markdown_output,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let options = parse_args(&args);

    match run_benchmark_suite(&options).await {
        Ok(results) => {
            let report = build_json_report(&results, &options);
            if let Some(path) = options.output_path.as_deref() {
                if let Err(err) = write_json_report(&report, path) {
                    eprintln!("Could not write JSON output: {}", err);
                }
            }
            if let Some(path) = options.markdown_output_path.as_deref() {
                let artifact_path = options.output_path.as_deref().unwrap_or("not written");
                if let Err(err) = write_markdown_output(&report, path, artifact_path) {
                    eprintln!("Could not write Markdown output: {}", err);
                }
            }
        }
        Err(err) => {
            eprintln!("Benchmark failed: {}", err);
            std::process::exit(1);
        }
    }
}
