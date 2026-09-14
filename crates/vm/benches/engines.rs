use criterion::{criterion_group, criterion_main, Criterion};
use vm::{execute, svm_execute, ExecutionContext};
use journal::StateJournal;

fn bench_evm_add(c: &mut Criterion) {
    // PUSH1 5, PUSH1 3, ADD, STOP
    let code = vec![0x60, 0x05, 0x60, 0x03, 0x01, 0x00];
    c.bench_function("evm_add", |b| {
        b.iter(|| {
            let ctx = ExecutionContext { code: code.clone(), sender: [0u8; 20], value: 0, journal: StateJournal::new() };
            execute(ctx, 1000).unwrap()
        })
    });
}

fn bench_evm_sstore(c: &mut Criterion) {
    // PUSH1 42, PUSH1 0, SSTORE, STOP
    let code = vec![0x60, 0x2a, 0x60, 0x00, 0x55, 0x00];
    c.bench_function("evm_sstore", |b| {
        b.iter(|| {
            let ctx = ExecutionContext { code: code.clone(), sender: [1u8; 20], value: 0, journal: StateJournal::new() };
            execute(ctx, 100_000).unwrap()
        })
    });
}

fn bench_svm_register_add(c: &mut Criterion) {
    // MOV_IMM r0, 5; MOV_IMM r1, 3; ADD r2, r0, r1; HALT
    let code = vec![
        0x02, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03, 0x02, 0x00, 0x01,
        0x00,
    ];
    c.bench_function("svm_register_add", |b| {
        b.iter(|| {
            svm_execute(&code, StateJournal::new(), 1000).unwrap()
        })
    });
}

fn bench_svm_account_write(c: &mut Criterion) {
    // MOV_IMM r0, 42; MOV_IMM r1, 0; WRITE_ACCOUNT r1, r0; HALT
    let code = vec![
        0x02, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x13, 0x01, 0x00,
        0x00,
    ];
    c.bench_function("svm_account_write", |b| {
        b.iter(|| {
            svm_execute(&code, StateJournal::new(), 100_000).unwrap()
        })
    });
}

criterion_group!(
    benches,
    bench_evm_add,
    bench_evm_sstore,
    bench_svm_register_add,
    bench_svm_account_write,
);
criterion_main!(benches);
