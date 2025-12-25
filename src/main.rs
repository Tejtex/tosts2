use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use clap::{Parser, Subcommand};
use tracing::{error, info};
use wait_timeout::ChildExt;

#[derive(Parser)]
#[command(name = "tosts")]
struct tosts {
    #[command(subcommand)]
    command: Commands,

    /// number of tests
    #[arg(short, long)]
    number: u64,

    /// time limit (in centiseconds)
    #[arg(short, long)]
    timelimit: u64,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// interactive test with interactor (NOT SUPPORTED)
    #[command(alias = "i")]
    Interactive {
        interactor: PathBuf,
        solution: PathBuf,
    },
    /// test solution with generator and another solution
    #[command(alias = "n")]
    Normal {
        generator: PathBuf,
        solution1: PathBuf,
        solution2: PathBuf,
    },
}

enum Verdict {
    OK,
    TLE,
    WA,
}

fn run_on_test(file: &PathBuf, test: &str, timeout: Duration) -> Result<String, Verdict> {
    let mut child = Command::new(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start process");

    {
        let stdin = child.stdin.as_mut().expect("Cannot open stdin");
        stdin.write_all(test.as_bytes()).expect("Failed to write to stdin");
    }

    match child.wait_timeout(timeout).expect("Error waiting for process") {
        Some(_) => {
            let output = child.wait_with_output().expect("Failed to get process output");
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }
        None => {
            child.kill().expect("Failed to kill process");
            Err(Verdict::TLE)
        }
    }
}

fn compare(sol1: &PathBuf, sol2: &PathBuf, test: &str, timeout: Duration) -> Verdict {
    let res1 = run_on_test(sol1, test, timeout);
    let res2 = run_on_test(sol2, test, timeout);

    match (res1, res2) {
        (Ok(a), Ok(b)) => if a.trim() == b.trim() { Verdict::OK } else { Verdict::WA },
        (Err(e), _) | (_, Err(e)) => e,
    }
}

fn gen_test(generator: &PathBuf) -> String {
    let output = Command::new(generator)
        .output()
        .expect("Failed to run generator");
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn test(
    sol1: PathBuf,
    sol2: PathBuf,
    timeout: Duration,
    generator: PathBuf,
    num: u64,
) {
    for i in 1..=num {
        let test_input = gen_test(&generator);
        let verdict = compare(&sol1, &sol2, &test_input, timeout);

        match verdict {
            Verdict::OK => info!("TEST {} OK", i),
            Verdict::WA => {
                error!("TEST {} WA", i);
                info!("Saving test to `test.in`");
                let mut file = File::create("test.in").expect("Failed to create file");
                file.write_all(test_input.as_bytes()).expect("Failed to write file");
                break;
            }
            Verdict::TLE => {
                error!("TEST {} TLE", i);
                info!("Saving test to `test.in`");
                let mut file = File::create("test.in").expect("Failed to create file");
                file.write_all(test_input.as_bytes()).expect("Failed to write file");
                break;
            }
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    let args = tosts::parse();
    info!("STARTING TESTS");

    match args.command {
        Commands::Normal { generator, solution1, solution2 } => {
            test(
                solution1,
                solution2,
                Duration::from_millis(args.timelimit * 10),
                generator,
                args.number,
            );
        }
        Commands::Interactive { .. } => panic!("Interactive tests not supported"),
    }
}
