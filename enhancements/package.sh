#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
if [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo 'Stage new enhancement files before creating the patch package.' >&2
    exit 1
fi
mkdir -p target
task_stage=$(mktemp -d "$task_root/target/enhancement-package.XXXXXX")
git diff --binary v0.30.1 -- . > "$task_stage/changes.patch"
cp enhancements/manifest.toml enhancements/README.txt LICENSE-MIT LICENSE-APACHE "$task_stage/"
(cd "$task_stage" && sha256sum changes.patch manifest.toml README.txt LICENSE-MIT LICENSE-APACHE > SHA256SUMS)
task_package="$task_root/target/solidcraft-enhancements-0.30.1-v12.tar.gz"
tar -C "$task_stage" -czf "$task_package" changes.patch manifest.toml README.txt LICENSE-MIT LICENSE-APACHE SHA256SUMS
printf '%s\n' "$task_package"
