#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/guix.py --github_access_token 03af38635985e17a62a49abcd5273cba24707794 "${@}"
