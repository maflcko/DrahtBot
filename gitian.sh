#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/gitian.py --github_access_token 3f0bf3de9cb834fb2929c98d6cb800d14ea1de85 "${@}"
