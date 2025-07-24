#!/usr/bin/env bash

FIXTURE="typos"
REPO_URL="https://github.com/crate-ci/typos"
VERSION="v1.34.0"
CLIPPY_LINTS=("" "-Wclippy::unnecessary_semicolon" "-Wclippy::map_unwrap_or" "-Wclippy::pedantic")

command=$1
base_dir="/tmp/benchsuite"
if [ -f ${@: -1} ]; then
  base_dir=${@: -1}
fi

log_path="${base_dir}/$FIXTURE.log"
fixture_dir="$base_dir/${FIXTURE}"

function download {
  git clone --branch $VERSION $REPO_URL $fixture_dir >> ${log_path}
}

function clear {
  rm -Rf ${fixture_dir} ${log_path}
}

case $command in
  path)
    echo $fixture_dir
    ;;
  clear)
    clear
    ;;
  download)
    download
    ;;
  count)
    echo ${#CLIPPY_LINTS[@]}
    ;;
  index)
    echo ${CLIPPY_LINTS[$2]}
    ;;
  *)
    >&2 echo "Invalid command: $command"
    exit 1
    ;;
esac

