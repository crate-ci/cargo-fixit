#!/usr/bin/env bash

set -e

base_dir="/tmp/benchsuite"
if [[ $# -ge 1 ]]; then
  base_dir=$1
fi

mkdir -p $base_dir
current_dir=`dirname $(readlink -f $0)`
commit_id=`git rev-parse --short HEAD`

log_file="$base_dir/fixit.log"
report_file="$base_dir/report.md"

rm -f $log_file $report_file

current_day=`date +%Y-%m-%d`
mkdir -p "$current_dir/runs"
final_report_file="$current_dir/runs/$current_day-$commit_id.md"
if [ -d $final_report_file ]; then
  echo "\`$final_report_file\` already exists"
  exit 1
fi

echo "Building \`cargo-fixit\`"
cargo build --release 2>> $log_file
if [ $? -ne 0 ]; then
  echo "Could not install cargo-fixit"
  echo "Try running the script from the project's root directory"
  exit 1
fi

build_path="$current_dir/../target/release"
if [ ! -f "$build_path/cargo-fixit" ]; then
  echo "Could not find release build"
  exit 1
fi

export PATH="$current_dir/../target/release:$PATH"

function benchmark_fixture {
  fixture_path=$1

  name=`eval $fixture_path name`
  repo=`eval $fixture_path path`
  count=`eval $fixture_path lints_count`
  fixture_log_file=`eval $fixture_path log`

  rm -f $fixture_log_file

  if [ ! -d "$repo" ]; then
    echo "Downloading \`$name\`"
    eval $fixture_path download_repo
  fi
  pushd $repo >> $fixture_log_file
  mkdir -p target

  echo -e "# $name\n" >> $report_file

  for i in $(seq 0 $(( $count - 1 )));
  do
    lint=`eval $fixture_path lint_index $i`
    echo "$(($i+1))/$count: Using RUSTFLAGS=\"$lint\""

    echo "## RUSTFLAG=\"$lint\"" >> $report_file
    echo -e "$lint\n" >> $log_file

    RUSTFLAGS="$lint" hyperfine --warmup 2 --show-output \
      --export-json target/out-$i.json \
      --export-markdown target/out-$i.md \
      "cargo clippy --workspace --all-targets" \
      "cargo fixit --clippy --workspace --all-targets" \
      "cargo clippy --fix --workspace --all-targets" \
      --conclude "git restore ." 2>> $fixture_log_file

    cat target/out-$i.md >> $report_file
    echo "" >> $report_file
  done

  echo -e "\n" >> $log_file

  popd >> $fixture_log_file
}

echo "" > $report_file
echo "# Fixit Benchmark" >> $report_file
echo "" >> $report_file
echo "These are the results as of commit $commit_id" >> $report_file
echo "" >> $report_file
echo "Command:" >> $report_file
echo "\`\`\`bash" >> $report_file
echo "$ $0 $base_dir $machine" >> $report_file
echo "\`\`\`" >> $report_file
echo "" >> $report_file
echo "" >> $report_file


typos_path="$current_dir/fixtures/typos.sh"
benchmark_fixture $typos_path

cp $report_file $final_report_file
