#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/coverage.py --github_access_token  03f190cd148675c1be607bfb9180d87a58ea768e "${@}"
