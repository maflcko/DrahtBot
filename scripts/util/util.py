from collections import defaultdict
import re
import os
import subprocess

def return_with_pull_metadata(get_pulls):
    pulls = get_pulls()
    pulls_update_mergeable = lambda: [p for p in pulls if p.mergeable is None and not p.merged]
    print('Fetching open pulls metadata ...')
    while pulls_update_mergeable():
        print('Update mergable state for pulls {}'.format([p.number for p in pulls_update_mergeable()]))
        [p.update() for p in pulls_update_mergeable()]
        pulls = get_pulls()
    return pulls


def calculate_table(base_folder, commit_folder, external_url, base_commit, commit):
    rows = defaultdict(lambda: ['', ''])  # map from abbrev file name to list of links
    for f in sorted(os.listdir(base_folder)):
        short_file_name = re.sub(r'(bitcoin-)?[a-f0-9]{12}', '*', f)
        os.chdir(base_folder)
        left = rows[short_file_name]
        left[0] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, base_commit, f)
        rows[short_file_name] = left

    for f in sorted(os.listdir(commit_folder)):
        short_file_name = re.sub(r'(bitcoin-)?[a-f0-9]{12}', '*', f)
        os.chdir(commit_folder)
        right = rows[short_file_name]
        right[1] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, commit, f)
        rows[short_file_name] = right

    text = ''
    for f in rows:
        text += '| {} | {} | {} |\n'.format(f, rows[f][0], rows[f][1])
    text += '\n'
    return text


def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)


def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()
