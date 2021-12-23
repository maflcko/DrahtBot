#!/usr/bin/env bash

source ./env_3/bin/activate && python3 ./scripts/rerun_ci.py --github_repos="bitcoin-core/gui:1p5t1ih7daolj7mfi1krp7339spo74pt2spqbod,bitcoin/bitcoin:34316ubgj58tnhd26mddqhm55bc5895pfaf79du" --github_access_token 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 "${@}"
