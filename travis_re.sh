#!/usr/bin/env bash

source ./env_3/bin/activate && python3 travis_re.py --github_access_token 21a15452da9a10e2ed5c0222bd9278d747ae2fea "${@}"
