#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
if [ -L target ]; then
    task_target=$(readlink -f target || readlink target)
    mkdir -p "$task_target"
fi
export CARGO_TARGET_DIR="$task_root/target"
export CARGO_PROFILE_CI_DEBUG=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
exec cargo build --locked --profile ci -p solidcraft
