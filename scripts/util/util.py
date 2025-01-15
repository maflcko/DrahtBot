import subprocess


def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)


def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()
