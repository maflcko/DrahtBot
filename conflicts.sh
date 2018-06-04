#!/usr/bin/env bash

#setup:
#virtualenv --python=python3 ./env_3
#pip install pygithub

source ./env_3/bin/activate && python3 conflicts.py --github_access_token ec10f501a7189c212874511056980b451c659274 --pull_id "${@}"
