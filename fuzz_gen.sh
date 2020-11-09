#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/fuzz_gen.py   "${@}"
