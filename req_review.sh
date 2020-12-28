#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/req_review.py --github_access_token 50a6eb204574a31ad86ddc9e57436aa775ff8bef "${@}"
