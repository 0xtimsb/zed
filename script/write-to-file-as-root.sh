#!/usr/bin/env bash

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <temp_file> <target_file>"
    exit 1
fi

temp_file="$1"
target_file="$2"

pkexec --disable-internal-agent tee "$target_file" < "$temp_file" > /dev/null
