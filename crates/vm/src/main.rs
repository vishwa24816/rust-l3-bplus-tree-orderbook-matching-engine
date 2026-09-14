use std::io::{self, Read};
use vm::{execute, ExecutionContext, svm_execute};
use journal::StateJournal;
use decoder::router::{EngineRouter, ExecutionEngine};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let (engine, code) = parse_args(&args);

    let journal = StateJournal::new();

    match engine {
        ExecutionEngine::Evm => {
            let ctx = ExecutionContext { code, sender: [0u8; 20], value: 0, journal };
            match execute(ctx, 10_000_000) {
                Ok(result) => {
                    if result.success {
                        if !result.output.is_empty() { println!("0x{}", hex::encode(&result.output)); }
                        eprintln!("evm  ok  gas_used={}", result.gas_used);
                    } else {
                        eprintln!("evm  revert  gas_used={}", result.gas_used);
                        std::process::exit(1);
                    }
                }
                Err(e) => { eprintln!("evm  error: {e}"); std::process::exit(1); }
            }
        }
        ExecutionEngine::Svm => {
            match svm_execute(&code, journal, 10_000_000) {
                Ok(result) => {
                    eprintln!("svm  ok  gas_used={}", result.gas_used);
                    if !result.journal.is_empty() {
                        eprintln!("  accounts written: {}", result.journal.accounts().len());
                    }
                }
                Err(e) => { eprintln!("svm  error: {e}"); std::process::exit(1); }
            }
        }
    }
}

fn parse_args(args: &[String]) -> (ExecutionEngine, Vec<u8>) {
    // -e svm:hexcode → SVM
    // -e hexcode    → EVM (default)
    // file.bin      → auto-detect from first byte
    // stdin         → auto-detect from first byte

    if args.len() == 2 && args[0] == "-e" {
        let input = &args[1];
        if let Some(hex) = input.strip_prefix("svm:") {
            return (ExecutionEngine::Svm, hex::decode(hex).expect("invalid hex"));
        }
        return (ExecutionEngine::Evm, hex::decode(input).expect("invalid hex"));
    }

    let bytes = if args.len() == 1 && std::path::Path::new(&args[0]).exists() {
        std::fs::read(&args[0]).expect("failed to read file")
    } else if args.is_empty() {
        let mut input = Vec::new();
        io::stdin().read_to_end(&mut input).expect("failed to read stdin");
        if input.is_empty() {
            eprintln!("usage: execution-engine -e <hex-bytecode>");
            eprintln!("       execution-engine -e svm:<hex-bytecode>");
            eprintln!("       execution-engine <file.bin>");
            eprintln!("       echo <hex> | execution-engine");
            std::process::exit(1);
        }
        let hex_str: String = input.iter().filter(|b| b.is_ascii_hexdigit()).map(|b| *b as char).collect();
        hex::decode(&hex_str).expect("invalid hex input")
    } else {
        eprintln!("usage: execution-engine -e <hex-bytecode>");
        eprintln!("       execution-engine -e svm:<hex-bytecode>");
        eprintln!("       execution-engine <file.bin>");
        std::process::exit(1);
    };

    let engine = EngineRouter::detect(&bytes);
    let code = EngineRouter::strip_prefix(&bytes).to_vec();
    (engine, code)
}
