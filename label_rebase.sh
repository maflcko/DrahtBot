#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/label_rebase.py --github_access_token d7d24ec6ff00000becdfbd15655202abc69e4d6f "${@}"
