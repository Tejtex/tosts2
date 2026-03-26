use anyhow::{Result, Context};
use std::{fs::File, fs, io::Write, path::PathBuf, process::{Command, Stdio}, time::Duration, thread, io};
use std::error::Error;
use std::io::{BufRead, BufReader, BufWriter, Read};
use std::process::{ChildStderr, ChildStdout};
use clap::{Parser, Subcommand};
use wait_timeout::ChildExt;
use indicatif::{ProgressStyle, ProgressBar, ProgressIterator};
use rayon::prelude::*;
use colored::Colorize;

#[derive(Parser)]
#[command(name = "tosts")]
struct Tosts {
    #[command(subcommand)]
    command: Commands,

}

#[derive(Subcommand, Clone)]
enum Commands {
    /// test solution with pregenerated test from a directory
    #[command(alias = "r")]
    Run {
        /// directory with input files
        #[arg(short, long)]
        in_dir: Option<PathBuf>,
        /// directory with output files
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
        // use the same directory for input and output
        #[arg(long)]
        io: Option<PathBuf>,
        /// extension of input files
        #[arg(alias="ie", long, default_value = "in")]
        in_ext: String,
        /// extension of output files
        #[arg(alias="oe", long, default_value = "out")]
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
        in_dir: Option<PathBuf>,
        /// directory with output files
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
        // use the same directory for input and output
        #[arg(long)]
        io: Option<PathBuf>,
        /// extension of input files
        #[arg(alias="ie", long, default_value = "in")]
        in_ext: String,
        /// extension of output files
        #[arg(alias="oe", long, default_value = "out")]
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
    RE,
}


fn get_pb(number: u64) -> ProgressBar {
    let pb = ProgressBar::new(number);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})").expect("SKIBIDI")
            .progress_chars("#>-")
    );
    pb
}

fn generate(in_dir: &PathBuf, out_dir: &PathBuf, solution: &PathBuf, generator: &PathBuf, in_ext: &String, out_ext: &String, number: u64) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| "couldn't create output directory")?;
    fs::create_dir_all(in_dir)
        .with_context(|| "couldn't create input directory")?;
    let pb = get_pb(number);
    (1..=number).into_par_iter().try_for_each(|i| -> Result<()> {
        let infile = in_dir.join(format!("{i}.{in_ext}"));
        let outfile = out_dir.join(format!("{i}.{out_ext}"));

        gen_test(&generator, &infile)
            .with_context(|| format!("couldn't generate test {i}"))?;

        let mut out = run_on_test_file(&solution, &infile, Duration::from_secs(100000))
            .with_context(|| format!("couldn't generate test {i}"))?.0.expect("shouldn't tle");

        let mut outfile = File::create(outfile)
            .with_context(|| "couldn't create output file")?;
        io::copy(&mut out, &mut outfile)
            .with_context(|| "couldn't copy output to output file")?;
        pb.inc(1);
        Ok(())
    })?;

    pb.finish_and_clear();
    eprintln!("{}", " ALL TESTS GENERATED ".on_green().black().bold());
    Ok(())

}

fn gen_test(generator: &PathBuf, outfile: &PathBuf) -> Result<()> {
    let file = File::create(outfile)
        .with_context(|| "couldn't create test file")?;

    Command::new(generator)
        .stdout(Stdio::from(file))
        .spawn()
        .with_context(|| "couldn't to execute process")?;
    Ok(())
}
fn run_on_test_file(file: &PathBuf, input_path: &PathBuf, timeout: Duration) -> Result<(Option<ChildStdout>, Verdict)> {
    let input_file = File::open(input_path).expect("Cannot open input file");

    let mut child = Command::new(file)
        .stdin(input_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| "couldn't spawn process")?;

    match child.wait_timeout(timeout).with_context(|| "couldn't wait for process")? {
        Some(status) => {
            if status.success() {
                Ok((Some(child.stdout.take().expect("")), Verdict::OK))
            } else {
                Ok((None, Verdict::RE))
            }
        }
        None => {
            let _ = child.kill();
            Ok((None, Verdict::TLE))
        }
    }
}

fn compare_bytes(r1: impl Read, r2: impl Read) -> bool {
    let mut b1 = BufReader::new(r1);
    let mut b2 = BufReader::new(r2);

    let mut buf1 = [0u8; 8192];
    let mut buf2 = [0u8; 8192];

    loop {
        let n1 = b1.read(&mut buf1).unwrap();
        let n2 = b2.read(&mut buf2).unwrap();

        if n1 != n2 {
            return false;
        }

        if n1 == 0 {
            return true;
        }

        if buf1[..n1] != buf2[..n2] {
            return false;
        }
    }
}

#[derive(Debug)]
struct VerdictError { input: String, i: u64, verdict: Verdict }
impl std::fmt::Display for VerdictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Test {} failed with {:?}", self.i, self.verdict)
    }
}
impl Error for VerdictError {}

fn run_from_dir(in_dir: PathBuf, out_dir: PathBuf, in_ext: String, out_ext: String, solution: PathBuf, timeout: Duration) -> Result<()> {
    
    let mut inputs = std::fs::read_dir(in_dir)
        .with_context(|| "couldn't read dir")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| *ext == *in_ext))
        .collect::<Vec<_>>();

    inputs.sort_by_key(|e| e.path());
    let pb = get_pb(inputs.len() as u64);

    match inputs.into_par_iter().enumerate().try_for_each(|(i, input_file)| -> Result<()> {
        
        let input_path = input_file.path();
        let output_file = out_dir
            .join(input_path.file_name().unwrap())
            .with_extension(&out_ext);

        let result = run_on_test_file(&solution, &input_path, timeout)?;
        match result.0 {
            Some(actual) => {
                let expected_file = File::open(&output_file).with_context(|| "couldn't open expected output")?;

                if !compare_bytes(actual, &expected_file) {
                    let input = std::fs::read_to_string(&input_path).with_context( || "couldn't read input")?;
                    return Err(anyhow::Error::new(VerdictError { input, i: (i + 1) as u64, verdict: Verdict::WA }));
                }
            },
            None => {
                let input = std::fs::read_to_string(&input_path).with_context( || "couldn't read input")?;
                return Err(anyhow::Error::new(VerdictError { input, i: (i + 1) as u64, verdict: result.1 }));

            }

        }


        pb.inc(1);
        Ok(())
    }) {
        Ok(_) => {
            pb.finish_and_clear();
            eprintln!("{}", " ALL TESTS PASSED ".on_green().black().bold());
            Ok(())
        },
        Err(e) => {
            if let Some(ve) = e.downcast_ref::<VerdictError>() {
                pb.finish_and_clear();

                let verdict_str = match ve.verdict {
                    Verdict::WA  => " WRONG ANSWER ".on_red().white().bold(),
                    Verdict::TLE => " TIME LIMIT EXCEEDED ".on_yellow().black().bold(),
                    Verdict::RE => " RUNTIME ERROR ".on_magenta().black().bold(),
                    Verdict::OK => unreachable!(),
                    
                };

                eprintln!("\n{} {}", verdict_str, format!("on test {}", ve.i).dimmed());
                eprintln!("{}", "─".repeat(40).dimmed());
                eprintln!("{}", "Input:".bold());

                let lines: Vec<&str> = ve.input.lines().collect();
                let preview = ve.input.chars().take(1000).collect::<String>();
                for line in preview.lines() {
                    eprintln!("  {}", line.cyan());
                }
                if ve.input.len() > 1000 {
                    eprintln!("  {}", "... (truncated)".dimmed());
                }
                eprintln!("{}", "─".repeat(40).dimmed());

                save_test(&ve.input, ve.i);
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn save_test(test: &str, i: u64) {
    let name = format!("fail_{}.in", i);
    eprintln!("{} {}", "Saved failing test to".dimmed(), name.bold());
    let mut file = File::create(&name).expect("Failed to create file");
    file.write_all(test.as_bytes()).expect("Failed to write file");
}
fn main() {

    let args = Tosts::parse();

    let result = match args.command {
        Commands::Run { in_dir, out_dir, io, in_ext, out_ext, solution, timelimit } => {

            let in_dir = in_dir
                .or_else(|| io.clone())
                .expect("Must provide --in-dir or --io");

            let out_dir = out_dir
                .or_else(|| io)
                .expect("Must provide --out-dir or --io");

            run_from_dir(
                in_dir,
                out_dir,
                in_ext,
                out_ext,
                solution,
                Duration::from_millis(timelimit * 10),
            )
        },
        Commands::Generate { in_dir, out_dir, io, in_ext, out_ext, solution, generator, number } => {

            let in_dir = in_dir
                .or_else(|| io.clone())
                .expect("Must provide --in-dir or --io");

            let out_dir = out_dir
                .or_else(|| io)
                .expect("Must provide --out-dir or --io");


            generate(&in_dir, &out_dir, &solution, &generator, &in_ext, &out_ext, number)
        }
        
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        for cause in e.chain().skip(1) {
            eprintln!("  {} {}", "caused by:".dimmed(), cause);
        }
        std::process::exit(1);
    }
}
