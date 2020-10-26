#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/coverage_fuzz.py  "${@}"
