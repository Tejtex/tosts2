
use std::{
    fs::File,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
    thread
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use clap::{Parser, Subcommand};
use tracing::{error, info};
use wait_timeout::ChildExt;
use std::sync::atomic::Ordering;
use clap::builder::TypedValueParser;

#[derive(Parser)]
#[command(name = "tosts")]
struct Tosts {
    #[command(subcommand)]
    command: Commands,

}
use indicatif::{ProgressStyle, ProgressBar};
use rayon::prelude::*;

#[derive(Subcommand, Clone)]
enum Commands {
    /// test solution with generator and another solution
    #[command(alias = "s")]
    Stress {
        generator: PathBuf,
        solution1: PathBuf,
        solution2: PathBuf,

        /// number of tests
        #[arg(short, long)]
        number: u64,

        /// time limit (in centiseconds)
        #[arg(short, long)]
        timelimit: u64,
    },

    /// test solution with pregenerated test from a directory
    #[command(alias = "r")]
    Run {
        /// directory with input files
        #[arg(short, long)]
        in_dir: PathBuf,
        /// directory with output files
        #[arg(short, long)]
        out_dir: PathBuf,
        /// extension of input files
        #[arg(alias="ie", long)]
        in_ext: String,
        /// extension of output files
        #[arg(alias="oe", long)]
        out_ext: String,

        /// time limit (in centiseconds)
        #[arg(short, long)]
        timelimit: u64,

        solution: PathBuf
    },

    /// generate tests with a generator and a solution
    #[command(alias = "g")]
    Generate {
        /// directory with input files
        #[arg(short, long)]
        in_dir: PathBuf,
        /// directory with output files
        #[arg(short, long)]
        out_dir: PathBuf,
        /// extension of input files
        #[arg(alias="ie", long)]
        in_ext: String,
        /// extension of output files
        #[arg(alias="oe", long)]
        out_ext: String,

        /// number of tests
        #[arg(short, long)]
        number: u64,

        generator: PathBuf,
        solution: PathBuf,


    }
}
#[derive(Debug)]
#[derive(PartialEq)]
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
            let output = child
                .stdout
                .take()
                .expect("stdout gone");
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::BufReader::new(output), &mut buf)
                .expect("read failed");
            Ok(buf)
        }
        None => {
            let _ = child.kill();
            Err(Verdict::TLE)
        }
    }
}

fn generate(in_dir: &PathBuf, out_dir: &PathBuf, solution: &PathBuf, generator: &PathBuf, in_ext: &String, out_ext: &String, number: u64) {
    info!("GENERATING {} TESTS", number);
    let pb = Arc::new(Mutex::new(ProgressBar::new(number)));
    pb.lock().unwrap().set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} ETA {eta}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    fs::create_dir_all(out_dir).expect("couldn't create output directory");
    fs::create_dir_all(in_dir).expect("couldn't create solution directory");
    (1..=number).into_par_iter().for_each(|i| {
        let test = gen_test(&generator);
        let infile = in_dir.join(format!("{i}.{in_ext}"));
        let outfile = out_dir.join(format!("{i}.{out_ext}"));

        fs::write(&infile, &test).expect("couldn't write input file");

        let out = run_on_test(&solution, &test, Duration::from_secs(100000))
            .expect("solution shouldn't TLE");

        fs::write(&outfile, out).expect("couldn't write output file");
        let pb = pb.lock().unwrap();
        pb.inc(1);
    });
    info!("ALL TESTS GENERATED");

}

fn compare(
    sol1: &PathBuf,
    sol2: &PathBuf,
    test: &str,
    timeout: Duration,
) -> Verdict {
    let t1 = thread::spawn({
        let sol1 = sol1.clone();
        let test = test.to_owned();
        move || run_on_test(&sol1, &test, timeout)
    });

    let t2 = thread::spawn({
        let sol2 = sol2.clone();
        let test = test.to_owned();
        move || run_on_test(&sol2, &test, timeout)
    });

    let res1 = t1.join().expect("thread panicked");
    let res2 = t2.join().expect("thread panicked");

    match (res1, res2) {
        (Ok(a), Ok(b)) => {
            if a.trim_end_matches(&['\n', '\r'][..])
                == b.trim_end_matches(&['\n', '\r'][..])
            {
                Verdict::OK
            } else {
                Verdict::WA
            }
        }
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
    let pb = Arc::new(Mutex::new(ProgressBar::new(num)));
    pb.lock().unwrap().set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} ETA {eta}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    let mut stop = AtomicBool::new(false);
    (1..=num).into_par_iter().for_each(|i| {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let test_input = gen_test(&generator);
        let verdict = compare(&sol1, &sol2, &*test_input, timeout);
        let pb = pb.lock().unwrap();
        if &Verdict::TLE == &verdict {
            save_test(&*test_input, i);
            pb.finish_and_clear();
            error!("TLE on test {i}");
            stop.store(true, Ordering::Relaxed);
        }
        if Verdict::WA == verdict {
            save_test(&*test_input, i);
            pb.finish_and_clear();
            error!("WA on test {i}");
            stop.store(true, Ordering::Relaxed);
        }

        pb.inc(1);
    });

    info!("ALL TESTS PASSED");
}
fn run_from_dir(in_dir: PathBuf, out_dir: PathBuf, in_ext: String, out_ext: String, solution: PathBuf, timeout: Duration) {
    let mut inputs = std::fs::read_dir(in_dir)
        .expect("Failed to read input dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| *ext == *in_ext))
        .collect::<Vec<_>>();

    inputs.sort_by_key(|e| e.path());

    let pb = Arc::new(Mutex::new(ProgressBar::new(inputs.len() as u64)));
    pb.lock().unwrap().set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} ETA {eta}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );

    let mut stop = AtomicBool::new(false);
    inputs.par_iter().for_each(|input_file| {

        if stop.load(Ordering::Relaxed) {
            return;
        }
        let input = std::fs::read_to_string(input_file.path()).expect("Cannot read input");
        let output_file = out_dir.join(
            input_file.path().file_name().unwrap()
        ).with_extension(&out_ext);

        let expected = std::fs::read_to_string(output_file)
            .expect("Cannot read expected output");

        let result = run_on_test(&solution, &input, timeout).unwrap_or_else(|v| match v {
            Verdict::TLE => {
                error!("TLE on {}", input_file.path().display());
                stop.store(true, Ordering::Relaxed);
                "".to_string()
            }
            _ => "".to_string(),
        });
        if stop.load(Ordering::Relaxed) {
            return;
        }

        if (&result).trim_end_matches(&['\n','\r'][..]) != (&expected).trim_end_matches(&['\n','\r'][..]) {
            error!("WA on {}", input_file.path().display());
            save_test(&input, 0);
            stop.store(true, Ordering::Relaxed);
        }
        let pb = pb.lock().unwrap();
        pb.inc(1);
    });

    info!("ALL TESTS PASSED");


}

fn save_test(test: &str, i: u64) {
    let name = format!("fail_{}.in", i);
    let mut file = File::create(&name).expect("Failed to create file");
    file.write_all(test.as_bytes()).expect("Failed to write file");
    info!("Saved failing test to {}", name);
}
fn main() {
    tracing_subscriber::fmt::init();

    let args = Tosts::parse();
    info!("STARTING");

    match args.command {
        Commands::Stress { generator, solution1, solution2, number, timelimit } => {
            test(
                solution1,
                solution2,
                Duration::from_millis(timelimit * 10),
                generator,
                number,
            );
        },
        Commands::Run { in_dir, out_dir, in_ext, out_ext, solution, timelimit } => {
            
            run_from_dir(in_dir, out_dir, in_ext, out_ext, solution, Duration::from_millis(timelimit * 10))
        },
        Commands::Generate {in_dir, out_dir, in_ext, out_ext, solution, generator, number} => {
            generate(&in_dir, &out_dir, &solution, &generator, &in_ext, &out_ext, number)
        }
        
    }
}
