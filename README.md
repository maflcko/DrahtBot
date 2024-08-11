Experimental bot scripts to help maintainers of large open source projects.
Also includes some Bitcoin related scripts.

example cmd
-----------

```
( cd rerun_ci && cargo run -- --help )
```

install (legacy python scripts only)
-------

```
virtualenv --python=python3 ./env_3
source ./env_3/bin/activate
pip install pygithub
#pip install github3.py
pip install mwclient
```
