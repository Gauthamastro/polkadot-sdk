// This file adds focused tests to help debug missing host-function imports
// by exercising `sc-executor-wasmtime` end-to-end with tiny WAT modules.

use sc_executor_common::{
    runtime_blob::RuntimeBlob,
    wasm_runtime::{HeapAllocStrategy, DEFAULT_HEAP_ALLOC_STRATEGY},
};
use sp_wasm_interface::{HostFunctions, Signature, ValueType};

type HostFns = sp_io::SubstrateHostFunctions;

fn semantics(instantiation: super::InstantiationStrategy) -> super::Semantics {
    super::Semantics {
        instantiation_strategy: instantiation,
        deterministic_stack_limit: None,
        canonicalize_nans: false,
        parallel_compilation: true,
        heap_alloc_strategy: DEFAULT_HEAP_ALLOC_STRATEGY,
        wasm_multi_value: false,
        wasm_bulk_memory: false,
        wasm_reference_types: false,
        wasm_simd: false,
    }
}

fn cfg(allow_missing: bool) -> super::Config {
    super::Config { allow_missing_func_imports: allow_missing, cache_path: None, semantics: semantics(super::InstantiationStrategy::RecreateInstance) }
}

fn wat_for_import(fn_name: &str, sig: &Signature) -> String {
    let param_list = sig
        .args
        .iter()
        .map(|t| match t {
            ValueType::I32 => "i32",
            ValueType::I64 => "i64",
            ValueType::F32 => "f32",
            ValueType::F64 => "f64",
        })
        .collect::<Vec<_>>()
        .join(" ");

    let result = match sig.return_value {
        Some(ValueType::I32) => " (result i32)",
        Some(ValueType::I64) => " (result i64)",
        Some(ValueType::F32) => " (result f32)",
        Some(ValueType::F64) => " (result f64)",
        None => "",
    };

    format!(
        r#"(module
            (import "env" "{fn_name}" (func (param {param_list}){result}))
            (memory 1)
            (export "memory" (memory 0))
            (global (export "__heap_base") i32 (i32.const 0))
            (func (export "main") (param i32 i32) (result i64)
                (i64.const 0)
            )
        )"#
    )
}

#[test]
fn detects_missing_imports_when_disallowed() {
    // Import a function that doesn't exist on the host and assert that
    // `create_runtime` returns a descriptive error mentioning the symbol.
    let wat = r#"(module
        (import "env" "definitely_not_a_real_host_fn" (func (param i32 i32)))
        (memory 1)
        (export "memory" (memory 0))
        (global (export "__heap_base") i32 (i32.const 0))
        (func (export "main") (param i32 i32) (result i64) (i64.const 0))
    )"#;

    let wasm = wat::parse_str(wat).expect("wat parsing should work");
    let blob = RuntimeBlob::uncompress_if_needed(&wasm).expect("blob");

    let err = super::create_runtime::<HostFns>(blob, cfg(false))
        .expect_err("runtime creation must fail if import is missing");

    let msg = format!("{err:?}");
    assert!(
        msg.contains("'env:definitely_not_a_real_host_fn'"),
        "missing import name should be reported; got: {msg}"
    );
}

#[test]
fn can_link_any_known_substrate_host_function() {
    // Pick a host function from `SubstrateHostFunctions` and construct a tiny module
    // that imports it (we never call it). This verifies that registration and linking work.
    let hf = HostFns::host_functions()
        .into_iter()
        .next()
        .expect("SubstrateHostFunctions must not be empty");

    let wat = wat_for_import(hf.name(), &hf.signature());
    let wasm = wat::parse_str(&wat).expect("wat parsing should work");
    let blob = RuntimeBlob::uncompress_if_needed(&wasm).expect("blob");

    // Linking should succeed when missing imports are not allowed because we provided one that exists.
    let _rt = super::create_runtime::<HostFns>(blob, cfg(false)).expect("linking host fn must succeed");
}

#[test]
fn print_runtime_imports_vs_exposed_host_functions() {
    // This is a lightweight debug aid: it prints the difference between the set of
    // imports required by the default test runtime and the set of exposed host functions.
    // Run with: `cargo test -p sc-executor-wasmtime -- --nocapture` to see output.
    let wasm = sc_runtime_test::wasm_binary_unwrap();
    let blob = RuntimeBlob::uncompress_if_needed(wasm).expect("blob");

    // Use the real creation path to ensure the same preparation steps are applied.
    // If there are missing imports, the error will list them. We capture and print it
    // to ease debugging instead of failing the test.
    match super::create_runtime::<HostFns>(blob, cfg(false)) {
        Ok(_) => {
            println!("All required runtime function imports are satisfied by the host.");
        }
        Err(e) => {
            println!("Runtime reports missing imports -> {e:?}");
        }
    }

    // Additionally, print the list of exposed host function names so they can be compared.
    let host_names: Vec<_> = HostFns::host_functions()
        .into_iter()
        .map(|f| f.name().to_string())
        .collect();
    println!(
        "Host exposes {} functions; first 10: {:?}",
        host_names.len(),
        &host_names.iter().take(10).collect::<Vec<_>>()
    );
}
