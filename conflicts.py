from github import Github
import sys
import time
import subprocess
import argparse
import os
import concurrent.futures

UPSTREAM_PULL = 'upstream-pull'


def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)


def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()


def git_fetch_branch(branch_name):
    call_git(['fetch', 'origin', '{}'.format(branch_name), '--quiet'])
    call_git(['checkout', '--quiet', 'FETCH_HEAD'])
    call_git(['checkout', '--quiet', '-B', '{}'.format(branch_name)])


def git_checkout(local_ref):
    call_git(['checkout', '--quiet', '{}'.format(local_ref)])


def calc_conflicts(pulls_mergeable, num, base_branch):
    conflicts = []
    base_id = get_git(['log', '-1', '--format=%H', base_branch])
    for i, pull_other in enumerate(pulls_mergeable):
        if num == pull_other.number:
            continue
        print('[{}/{}] Checking for conflicts {} <> {} <> {} ... '.format(i, len(pulls_mergeable), base_branch, num, pull_other.number), end=' ', flush=True)
        call_git(['checkout', base_id, '--quiet'])
        try:
            call_git(['merge', '{}/{}/head'.format(UPSTREAM_PULL, num), '{}/{}/head'.format(UPSTREAM_PULL, pull_other.number), '-m', 'Octomerge {}+{}+{}'.format(base_branch, num, pull_other.number), '--quiet'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print('OK')
        except subprocess.CalledProcessError:
            call_git(['merge', '--abort'])
            conflicts += [pull_other]
            print('CONFLICT')
    return conflicts


def update_comment(dry_run, login_name, pull, pulls_conflict):
    if not pulls_conflict:
        return

    ID_CONFLICTS_COMMENT = '<!--e57a25ab6845829454e8d69fc972939a-->'

    text = ID_CONFLICTS_COMMENT
    text += 'The following pull requests conflict with this one:\n'
    text += ''.join(['\n* #{} ({})'.format(p.number, p.title.strip()) for p in pulls_conflict])
    text += '\n\n'
    text += 'If you consider this pull request important, please also help to review the conflicting pull requests. '
    text += 'Ideally, start with the one that should be merged first.'

    for c in pull.get_issue_comments():
        if c.user.login == login_name:
            if c.body.startswith(ID_CONFLICTS_COMMENT):
                if dry_run:
                    print('{}.{}.body = {}'.format(pull, c, text))
                else:
                    c.edit(text)
                return

    if dry_run:
        print('{}.new_comment.body = {}'.format(pull, text))
    else:
        pull.create_issue_comment(text)


def main():
    parser = argparse.ArgumentParser(description='Determine conflicting pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--pull_id', type=int, help='The pull request to check conflicts against.', default=0)
    parser.add_argument('--update_comments', action='store_true', help='Update the "conflicts comments".', default=False)
    parser.add_argument('--git_repo', help='The locally cloned git repo used for scratching', default=os.path.abspath(os.path.dirname(os.path.realpath(__file__)) + '/bitcoin_git'))
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    print('Update git repo {}'.format(args.git_repo))
    os.chdir(args.git_repo)
    git_fetch_branch(args.base_name)
    call_git(['checkout', 'origin/{}'.format(args.base_name), '--quiet'])
    call_git(['checkout', '-B', args.base_name, '--quiet'])
    call_git(['diff', '--exit-code'])  # Exit on changes

    print('Fetching open pulls ...')
    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)
    pulls = github_repo.get_pulls(state='open')
    print('Fetching open pulls metadata ...')
    pulls = [p for p in pulls if p.base.ref == args.base_name]
    pulls_update_mergeable = lambda: [p for p in pulls if p.mergeable is None]
    while pulls_update_mergeable():
        print('Update mergable state for pulls {}'.format([p.number for p in pulls_update_mergeable()]))
        [p.update() for p in pulls_update_mergeable()]

    print('Open {}-pulls: {}'.format(args.base_name, len(pulls)))
    pulls_mergeable = [p for p in pulls if p.mergeable]
    print('Open mergeable {}-pulls: {}'.format(args.base_name, len(pulls_mergeable)))

    print('Fetching diffs ...')
    call_git(['fetch', UPSTREAM_PULL])

    if args.update_comments:
        for pull_update in pulls_mergeable:
            if pull_update.number < 13385:
                # For now
                continue
            pulls_conflict = calc_conflicts(pulls_mergeable=pulls_mergeable, num=pull_update.number, base_branch=args.base_name)
            update_comment(dry_run=args.dry_run, login_name=github_api.get_user().login, pull=pull_update, pulls_conflict=pulls_conflict)

    if args.pull_id:
        pull_merge = [p for p in pulls if p.number == args.pull_id]

        if not pull_merge:
            print('{} not found in all {} open {} pulls'.format(args.pull_id, len(pulls), args.base_name))
            sys.exit(-1)
        pull_merge = pull_merge[0]

        if not pull_merge.mergeable:
            print('{} is not mergeable'.format(pull_merge.number))
            sys.exit(-1)

        conflicts = calc_conflicts(pulls_mergeable=pulls_mergeable, num=pull_merge.number, base_branch=args.base_name)

        print()
        print('{} conflicts for pull #{} ({}):'.format(len(conflicts), pull_merge.number, pull_merge.title))
        for pull_conflict in conflicts:
            print('#{} ({})'.format(pull_conflict.number, pull_conflict.title))

        print()
        print('Needs rebase due to merge of #{}'.format(pull_merge.number))


if __name__ == '__main__':
    main()
