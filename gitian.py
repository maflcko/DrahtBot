from github import Github, GithubException
import time
import argparse
import os
import sys
import tempfile
import subprocess

from util.util import return_with_pull_metadata, call_git, get_git

ID_GITIAN_COMMENT = '<!--a722867cd34abeea1fadc8d60700f111-->'
UPSTREAM_PULL = 'upstream-pull'


def main():
    parser = argparse.ArgumentParser(description='Gitian build and create an issue comment to share the results.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    parser.add_argument('--gitian_folder', help='The local scratch folder for temp gitian results', default=os.path.abspath(os.path.dirname(os.path.realpath(__file__)) + '/scratch_gitian'))
    parser.add_argument('--gitian_jobs', help='The number of jobs', default=2)
    parser.add_argument('--gitian_mem', help='The memory to use', default=2000)
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    print()
    print('Make sure to install docker and run the https://docs.docker.com/install/linux/linux-postinstall/')
    print('sudo groupadd docker ; sudo usermod -aG docker $USER')
    print()
    url = 'https://github.com/{}'.format(args.github_repo)

    temp_dir = os.path.join(args.gitian_folder, '')
    os.makedirs(temp_dir, exist_ok=True)
    git_repo_dir = os.path.join(temp_dir, 'bitcoin')

    def call_gitian_build(args_fwd, *, signer='none_signer', commit=None):
        os.chdir(git_repo_dir)
        call_git(['checkout', commit])
        os.chdir(temp_dir)
        subprocess.check_call([
            sys.executable,
            '{}'.format(os.path.join(temp_dir, '..', 'gitian-build.py')),
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

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_gitian = github_repo.get_label('Needs gitian build')

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
    call_git(['fetch', '--quiet', UPSTREAM_PULL])
    print('Get open, mergeable {} pulls ...'.format(args.base_name))
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])
    call_git(['fetch', '--quiet', UPSTREAM_PULL])  # Do it again just to be safe
    call_git(['fetch', 'origin'])
    base_commit = get_git(['log', '-1', '--format=%H', 'origin/{}'.format(args.base_name)])
    pulls = [p for p in pulls if p.base.ref == args.base_name]
    pulls = [p for p in pulls if p.mergeable]

    print('Num: {}'.format(len(pulls)))

    if not os.path.isdir(os.path.join(temp_dir, 'gitian-builder')):
        print('Setting up docker gitian ...')
        call_gitian_build(['--setup'], commit=base_commit)
        os.chdir(os.path.join(temp_dir, 'gitian-builder'))
        call_git(['apply', '../../gitian_builder_gbuild.patch'])

    print('Starting gitian build for base branch ...')
    call_gitian_build(['--build', '--commit'], commit=base_commit)
    base_folder = os.path.join(temp_dir, 'bitcoin-binaries', base_commit)

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

        print('{}\n    .remove_from_labels({})'.format(p, label_needs_gitian))
        comments = [c for c in issue.get_comments() if c.body.startswith(ID_GITIAN_COMMENT)]
        print('    + delete {} comments'.format(len(comments)))

        print(sorted(os.listdir(base_folder)))
        print(sorted(os.listdir(commit_folder)))

        if not args.dry_run:
            issue.remove_from_labels(label_needs_gitian)
            for c in comments:
                c.delete()


if __name__ == '__main__':
    main()
