#!/usr/bin/env bash

source ./env_3/bin/activate && python3 conflicts.py --update_comments --github_access_token  b6005448c263761141325e7bf756d14ce06ce34d "$@"
