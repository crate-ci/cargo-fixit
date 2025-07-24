#!/usr/bin/env bash

set -e

base_dir="/tmp/benchsuite"
if [[ $# -ge 1 ]]; then
  base_dir=$1
fi
mkdir -p $base_dir
current_dir=`dirname $(readlink -f $0)`

function bench {
  path=$1

  repo=`eval $path path`
  count=`eval $path count`
  if [ ! -d "$repo" ]; then
    eval $path download
  fi
  pushd $repo
  mkdir -p target

  for i in $(seq 0 $(( $count - 1 )));
  do
    lint=`eval $path index $i`
    RUSTFLAGS="$lint" hyperfine --warmup 1 -i --show-output \
      --export-json target/out-$i.json \
      --export-markdown target/out-$i.md \
      "cargo clippy --workspace --all-targets" \
      "cargo clippy --fix --allow-staged --workspace --all-targets" \
      "cargo fixit --clippy --allow-staged --workspace --all-targets" \
      --conclude "git restore ."
  done

  popd
}

typos_path="$current_dir/fixtures/typos.sh"
bench $typos_path
