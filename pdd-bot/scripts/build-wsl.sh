#!/bin/bash


get_script_dir () {
     SOURCE="${BASH_SOURCE[0]}"
     # While $SOURCE is a symlink, resolve it
     while [ -h "$SOURCE" ]; do
          DIR="$( cd -P "$( dirname "$SOURCE" )" && pwd )"
          SOURCE="$( readlink "$SOURCE" )"
          # If $SOURCE was a relative symlink (so no "/" as prefix, need to resolve it relative to the symlink base directory
          [[ $SOURCE != /* ]] && SOURCE="$DIR/$SOURCE"
     done
     DIR="$( cd -P "$( dirname "$SOURCE" )" && pwd )"
     echo "$DIR"
}

SCRIPTDIR=$(get_script_dir)

echo "build version $(git describe --tags --always --dirty)"

cargo build --release --target x86_64-unknown-linux-gnu --manifest-path $SCRIPTDIR/../Cargo.toml --target-dir $SCRIPTDIR/../target/target-wsl
#cargo build --release --target x86_64-unknown-linux-musl --manifest-path $SCRIPTDIR/../Cargo.toml --target-dir $SCRIPTDIR/../target/target-wsl
if [[ $? != 0 ]]; then
    echo "FAIL"
    exit 1
fi
