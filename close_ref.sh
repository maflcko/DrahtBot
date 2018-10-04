#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/close_ref.py --github_access_token 4103fa7a1326465c05a0be58d43cece224aa1311 "${@}"
