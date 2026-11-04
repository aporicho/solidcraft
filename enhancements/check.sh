#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
export CARGO_TARGET_DIR="$task_root/target"
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_CI_DEBUG=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
export SOLIDCRAFT_TEST_JOBS="${SOLIDCRAFT_TEST_JOBS:-4}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-2}"
export ACTION_MATRIX_JOBS="${ACTION_MATRIX_JOBS:-2}"
exec cargo xtask ci
