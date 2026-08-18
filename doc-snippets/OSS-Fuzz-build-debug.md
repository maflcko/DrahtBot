https://google.github.io/oss-fuzz/advanced-topics/reproducing/


```
### echo no_podman && dnf install git vim htop podman-docker python3 screen  -y  && export PROJECT_NAME=bitcoin-core  && git clone https://github.com/google/oss-fuzz && cd oss-fuzz

apt update && apt install git vim htop docker.io screen python-is-python3 -y  && export PROJECT_NAME=bitcoin-core  && git clone https://github.com/google/oss-fuzz && cd oss-fuzz


rm -rf build/out/bitcoin-core/* && python3 infra/helper.py build_fuzzers --sanitizer undefined  --engine libfuzzer $PROJECT_NAME && python3 infra/helper.py check_build  --sanitizer undefined  --engine libfuzzer $PROJECT_NAME


infra/base-images/all.sh
python3 infra/helper.py build_image base-builder  &&  python3 infra/helper.py build_image base-runner
