#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/conflicts.py --github_access_token ec10f501a7189c212874511056980b451c659274 --pull_id "${@}"
