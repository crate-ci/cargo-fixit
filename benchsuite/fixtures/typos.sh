#!/usr/bin/env bash

FIXTURE="typos"
REPO_URL="https://github.com/crate-ci/typos"
VERSION="v1.34.0"
CLIPPY_LINTS=( \
  "" \
  "-Wclippy::unnecessary_semicolon" \
  "-Wclippy::map_unwrap_or" \
  "-Wclippy::pedantic" \
)

command=$1
base_dir="/tmp/benchsuite"
if [ -f ${@: -1} ]; then
  base_dir=${@: -1}
fi

log_file="$base_dir/$FIXTURE.log"
fixture_dir="$base_dir/$FIXTURE"

function download_repo {
  git clone --branch $VERSION $REPO_URL $fixture_dir 2>> ${log_file}
}

function clear {
  rm -Rf ${fixture_dir} ${log_file}
}

case $command in
  name)
    echo $FIXTURE
    ;;
  path)
    echo $fixture_dir
    ;;
  clear)
    clear
    ;;
  download_repo)
    download_repo
    ;;
  lints_count)
    echo ${#CLIPPY_LINTS[@]}
    ;;
  lint_index)
    echo ${CLIPPY_LINTS[$2]}
    ;;
  log)
    echo $log_file
    ;;
  *)
    >&2 echo "Invalid command: $command"
    exit 1
    ;;
esac

