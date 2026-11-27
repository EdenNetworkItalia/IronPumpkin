#!/usr/bin/env bash
# Runs cargo in one of two build slots that every checkout and worktree of this repository
# shares, so parallel agents never run more than two cargo commands at once. The workstation has
# 62 GB of RAM, and 32 parallel linkers at 2-3 GB each ran it out of memory.
#
#   tools/cargo-slot.sh nextest run -p ironpumpkin-mods --lib --test-threads 4
set -u
common=$(git rev-parse --path-format=absolute --git-common-dir) || exit 1
locks="$(dirname "$common")/target/cargo-slots"
# Read-only opens: a sandboxed agent in a worktree may not write to the main checkout.
[ -e "$locks/slot2" ] || { mkdir -p "$locks" && touch "$locks/slot1" "$locks/slot2"; } || exit 1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-5}" RUST_MIN_STACK="${RUST_MIN_STACK:-268435456}"
# rustfmt compiles nothing and needs little memory, so it does not wait for a slot.
[ "${1:-}" = fmt ] && exec cargo "$@"
start=$SECONDS
while :; do
    for slot in 1 2; do
        exec 9<"$locks/slot$slot"
        # cargo and its children inherit fd 9, so the slot stays taken until they all exit.
        flock -n 9 && exec cargo "$@"
    done
    # The lock files are read-only for sandboxed agents, so there is no queue file. A command that
    # waits longer polls more often and so takes a free slot first: every 2 s, every 1 s after 5
    # minutes, every 0.2 s after 10 minutes.
    waited=$(( SECONDS - start ))
    sleep "$(( waited >= 600 ? 0 : 2 - waited / 300 )).$(( waited >= 600 ? 2 : 0 ))"
done
