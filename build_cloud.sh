#!/bin/bash

cargo build --release --target=wasm32-wasi --features ecp
cd ecp
fastly compute pack -p ../target/wasm32-wasi/release/rs_pbrt_ecp.wasm
fastly compute deploy