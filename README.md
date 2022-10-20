Experimental bot scripts to help maintainers of large open source projects.
Also includes some Bitcoin related scripts.

install
-------

```
virtualenv --python=python3 ./env_3
source ./env_3/bin/activate
pip install pygithub
#pip install github3.py
pip install mwclient
```

example cmd
-----------

```
while sleep 3600; do ./gitian.sh --gitian_jobs 1 --gitian_mem 2000 --domain https://drahtbot.space; done
```
