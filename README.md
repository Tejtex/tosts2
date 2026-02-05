# tosts

`tosts` is a lightweight **local Rust judge CLI** for competitive programming.

## installation
To install:
```bash
cargo install tosts
```

## usage
### Stress testing (Brute Mode)
Compares the main solution with a brute solution using a generator:
```bash
tosts stress \
   --number 1000 \
   --timelimit 100 \
   ./gen \
   ./sol1 \
   ./sol2
```
- the timelimit is in centiseconds (100 centiseconds = 1 second)
- after a **WA** or **TLE** it saves the test and stops
### Package testing (Run Mode)
Tests the solution using pregenerated tests:
```bash
tosts run \
   --in-dir tests/in \
   --out-dir tests/out \
   --in-ext in \
   --out-ext out \
   --timelimit 100 \
   ./sol
```
- the default `in-ext` is `in` and default `out-ext` is `out`
### Generating a package (Generate Mode)
Generates a test package that can be used to test a solution using the run mode:
```bash
tosts generate \
   --in-dir tests/in \
   --out-dir tests/out \
   --in-ext in \
   --out-ext out \
   --number 1000 \
   ./gen \
   ./sol
```