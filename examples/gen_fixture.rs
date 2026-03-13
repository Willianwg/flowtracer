#!/usr/bin/env -S cargo +nightly -Zscript
//! Fast log fixture generator for performance benchmarking.
//! Usage: cargo run --release --example gen_fixture -- [output_file] [target_mb]

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

const LEVELS: &[&str] = &["INFO", "DEBUG", "WARN", "ERROR"];
const FUNCTIONS: &[&str] = &[
    "CreateOrderController",
    "GetUser",
    "GetCart",
    "CreateInvoice",
    "GetProvider",
    "ListProductsController",
    "GetProducts",
    "ValidateInput",
    "ProcessPayment",
    "SendNotification",
    "UpdateInventory",
    "AuthMiddleware",
    "RateLimiter",
    "CacheService",
    "DatabaseQuery",
    "ExternalAPICall",
];
const ERRORS: &[&str] = &[
    "Connection refused",
    "Timeout after 30s",
    "No provider found with name \"paypau\"",
    "NullPointerException: Cannot invoke method on null",
    "Out of memory",
    "Permission denied",
    "Record not found",
    "Validation failed: invalid input",
];

fn main() {
    let args: Vec<String> = env::args().collect();
    let output = args.get(1).map(|s| s.as_str()).unwrap_or("benches/big.log");
    let target_mb: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
    let target_bytes = target_mb * 1024 * 1024;

    eprintln!("Generating ~{}MB log fixture → {}", target_mb, output);

    let file = File::create(output).expect("failed to create output file");
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);

    let mut rng = SimpleRng::new(42);
    let mut current_bytes: usize = 0;
    let mut req_counter: u64 = 0;
    let mut line_counter: u64 = 0;

    let base_hour = 10u32;
    let base_min = 0u32;
    let base_sec = 0u32;

    while current_bytes < target_bytes {
        req_counter += 1;
        let num_events = (rng.next() % 8) + 3;

        for e in 0..num_events {
            line_counter += 1;
            let sec_offset = (line_counter / 100) as u32;
            let h = base_hour + (base_sec + sec_offset) / 3600;
            let m = base_min + ((base_sec + sec_offset) % 3600) / 60;
            let s = (base_sec + sec_offset) % 60;
            let ms = rng.next() % 1000;

            let line = if e == num_events - 1 && rng.next() % 5 == 0 {
                let err = ERRORS[(rng.next() as usize) % ERRORS.len()];
                format!(
                    "2026-03-12 {:02}:{:02}:{:02}.{:03} [ERROR] RequestId=req-{:06} {}",
                    h, m, s, ms, req_counter, err
                )
            } else if e == 0 || rng.next() % 3 == 0 {
                let func = FUNCTIONS[(rng.next() as usize) % FUNCTIONS.len()];
                format!(
                    "2026-03-12 {:02}:{:02}:{:02}.{:03} [INFO] RequestId=req-{:06} Executing {}",
                    h, m, s, ms, req_counter, func
                )
            } else {
                let level = LEVELS[(rng.next() as usize) % 2];
                let func = FUNCTIONS[(rng.next() as usize) % FUNCTIONS.len()];
                format!(
                    "2026-03-12 {:02}:{:02}:{:02}.{:03} [{}] RequestId=req-{:06} {} processing step {}",
                    h, m, s, ms, level, req_counter, func, e
                )
            };

            current_bytes += line.len() + 1;
            writeln!(writer, "{}", line).expect("write failed");
        }
    }

    writer.flush().expect("flush failed");

    let actual_mb = current_bytes / (1024 * 1024);
    eprintln!(
        "Generated: {}MB, {} lines, {} requests",
        actual_mb, line_counter, req_counter
    );
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}
