from github import Github, GithubException
import time
import itertools
from collections import defaultdict
import shutil
import argparse
import os
import sys
import tempfile
import subprocess

from util.util import return_with_pull_metadata, call_git, get_git

ID_GITIAN_COMMENT = '<!--a722867cd34abeea1fadc8d60700f111-->'
UPSTREAM_PULL = 'upstream-pull'


def calculate_diffs(folder_1, folder_2):
    EXTENSIONS = ['.yml', '.log']
    files = set(os.listdir(folder_1)).intersection(set(os.listdir(folder_2)))
    files = [f for f in files if any(f.endswith(e) for e in EXTENSIONS)]
    for f in files:
        os.chdir(folder_2)
        file_1 = str(os.path.join(folder_1, f))
        file_2 = str(os.path.join(folder_2, f))
        subprocess.call('diff --color {} {} > {}.diff'.format(file_1, file_2, f), shell=True)


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Gitian build and create an issue comment to share the results.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    parser.add_argument('--gitian_folder', help='The local scratch folder for temp gitian results', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'gitian'))
    parser.add_argument('--gitian_jobs', help='The number of jobs', default=2)
    parser.add_argument('--gitian_mem', help='The memory to use', default=2000)
    parser.add_argument('--domain', help='Where the assets are reachable', default='http://127.0.0.1')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    parser.add_argument('--build_one_commit', help='Only build this one commit and exit.', default='')
    args = parser.parse_args()

    print()
    print('Make sure to install docker and run the https://docs.docker.com/install/linux/linux-postinstall/')
    print('sudo usermod -aG docker $USER')
    print()
    print('sudo usermod -aG www-data $USER')
    print('sudo chown -R www-data:www-data /var/www')
    print('sudo chmod -R g+rw /var/www')
    print('mv /var/www/html/index.html /tmp/')
    print('# Then reboot')
    print()
    url = 'https://github.com/{}'.format(args.github_repo)
    GITIAN_WWW_FOLDER = '/var/www/html/gitian/{}/'.format(args.github_repo)
    external_url = '{}/gitian/{}/'.format(args.domain, args.github_repo)

    if not args.dry_run:
        print('Clean gitian folder of old files')
        subprocess.check_call('find {} -mindepth 1 -maxdepth 1 -type d -ctime +{} | xargs rm -rf'.format(GITIAN_WWW_FOLDER, 15), shell=True)
        os.makedirs(GITIAN_WWW_FOLDER, exist_ok=True)

    temp_dir = os.path.join(args.gitian_folder, '')
    os.makedirs(temp_dir, exist_ok=True)
    git_repo_dir = os.path.join(temp_dir, 'bitcoin')

    def call_gitian_build(args_fwd, *, signer='none_signer', commit=None):
        os.chdir(git_repo_dir)
        call_git(['checkout', '--quiet', commit])
        os.chdir(temp_dir)
        subprocess.check_call([
            sys.executable,
            '{}'.format(os.path.join(THIS_FILE_PATH, 'gitian-build.py')),
            '--docker',
            '--jobs',
            '{}'.format(args.gitian_jobs),
            '--memory',
            '{}'.format(args.gitian_mem),
            '--url',
            '{}'.format(url),
            '--no-commit',
            '--commit',
            signer,
            commit,
        ] + args_fwd)

    if not os.path.isdir(git_repo_dir):
        print('Clone {} repo to {}/bitcoin'.format(url, temp_dir))
        os.chdir(temp_dir)
        call_git(['clone', '--quiet', url, 'bitcoin'])
        print('Set git metadata')
        os.chdir(git_repo_dir)
        with open(os.path.join(git_repo_dir, '.git', 'config'), 'a') as f:
            f.write('[remote "{}"]\n'.format(UPSTREAM_PULL))
            f.write('    url = {}\n'.format(url))
            f.write('    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n')
            f.flush()
        call_git(['config', 'user.email', 'no@ne.nl'])
        call_git(['config', 'user.name', 'none'])
    print('Fetch upsteam pulls')
    os.chdir(git_repo_dir)
    call_git(['fetch', '--quiet', '--all'])

    if not os.path.isdir(os.path.join(temp_dir, 'gitian-builder')):
        print('Setting up docker gitian ...')
        call_gitian_build(['--setup'], commit='HEAD')
        os.chdir(os.path.join(temp_dir, 'gitian-builder'))
        call_git(['apply', os.path.join(THIS_FILE_PATH, 'gitian_builder_gbuild.patch')])
        inputs_folder = os.path.join(temp_dir, 'gitian-builder', 'inputs', '')
        os.makedirs(inputs_folder, exist_ok=True)
        # Bitcoin Core before 0.20.0
        subprocess.check_call(['cp', os.path.join(THIS_FILE_PATH, 'MacOSX10.11.sdk.tar.gz'), inputs_folder])
        # Bitcoin Core after and including 0.20.0
        subprocess.check_call(['cp', os.path.join(THIS_FILE_PATH, 'MacOSX10.14.sdk.tar.gz'), inputs_folder])

    if args.build_one_commit:
        print('Starting gitian build for one commit ({}) ...'.format(args.build_one_commit))
        call_gitian_build(['--build', '--commit'], commit=args.build_one_commit)
        print('See folder:\n{}'.format(os.path.join(temp_dir, 'bitcoin-binaries', args.build_one_commit)))
        print('Exit')
        return

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_gitian = github_repo.get_label('Needs gitian build')

    print('Get open, mergeable {} pulls ...'.format(args.base_name))
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open', base=args.base_name)])
    call_git(['fetch', '--quiet', '--all'])  # Do it again just to be safe
    call_git(['fetch', '--quiet', 'origin'])
    base_commit = get_git(['log', '-1', '--format=%H', 'origin/{}'.format(args.base_name)])
    pulls = [p for p in pulls if p.mergeable]

    print('Num: {}'.format(len(pulls)))

    for i in [p.as_issue() for p in pulls]:
        if label_needs_gitian in i.get_labels():
            break
    else:
        print('Nothing tagged with {}. Exiting...'.format(label_needs_gitian.name))
        return

    print('Starting gitian build for base branch ...')
    call_gitian_build(['--build', '--commit'], commit=base_commit)
    base_folder = os.path.join(temp_dir, 'bitcoin-binaries', base_commit)
    if not args.dry_run:
        print('Moving results of {} to {}'.format(base_commit, GITIAN_WWW_FOLDER))
        shutil.rmtree(os.path.join(GITIAN_WWW_FOLDER, base_commit), ignore_errors=True)
        base_folder = shutil.move(src=base_folder, dst=GITIAN_WWW_FOLDER)

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        issue = p.as_issue()
        if label_needs_gitian not in issue.get_labels():
            continue

        print('Starting gitian build ...')
        os.chdir(git_repo_dir)
        commit = get_git(['log', '-1', '--format=%H', '{}/{}/merge'.format(UPSTREAM_PULL, p.number)])
        call_gitian_build(['--build', '--commit'], commit=commit)
        commit_folder = os.path.join(temp_dir, 'bitcoin-binaries', commit)
        if not args.dry_run:
            print('Moving results of {} to {}'.format(base_commit, GITIAN_WWW_FOLDER))
            shutil.rmtree(os.path.join(GITIAN_WWW_FOLDER, commit), ignore_errors=True)
            commit_folder = shutil.move(src=commit_folder, dst=GITIAN_WWW_FOLDER)

        calculate_diffs(base_folder, commit_folder)

        text = ID_GITIAN_COMMENT
        text += '\n'
        text += '### Gitian builds\n\n'
        text += '| File '
        text += '| commit {}<br>({}) '.format(base_commit, args.base_name)
        text += '| commit {}<br>({} and this pull) '.format(commit, args.base_name)
        text += '|\n'
        text += '|--|--|--|\n'

        text += calculate_table(base_folder, commit_folder, external_url, base_commit, commit)

        print('{}\n    .remove_from_labels({})'.format(p, label_needs_gitian))
        print('    .create_comment({})'.format(text))

        if not args.dry_run:
            issue.create_comment(text)
            issue.remove_from_labels(label_needs_gitian)


def calculate_table(base_folder, commit_folder, external_url, base_commit, commit):
    rows = defaultdict(lambda: ['', ''])  # map from file name to list of links
    for f in sorted(os.listdir(base_folder)):
        os.chdir(base_folder)
        left = rows[f]
        left[0] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, base_commit, f)
        rows[f] = left

    for f in sorted(os.listdir(commit_folder)):
        os.chdir(commit_folder)
        right = rows[f]
        right[1] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, commit, f)
        rows[f] = right

    text = ''
    for f in rows:
        text += '| {} | {} | {} |\n'.format(f, rows[f][0], rows[f][1])
    text += '\n'
    return text


if __name__ == '__main__':
    main()
