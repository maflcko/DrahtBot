from github import Github
import sys
import time
import subprocess
import argparse
import os
import tempfile

from util.util import return_with_pull_metadata, call_git, get_git

UPSTREAM_PULL = 'upstream-pull'


def calc_conflicts(pulls_mergeable, num, base_branch):
    conflicts = []
    base_id = get_git(['log', '-1', '--format=%H', 'origin/{}'.format(base_branch)])
    call_git(['checkout', base_id, '--quiet'])
    call_git(['merge', '--quiet', '{}/{}/head'.format(UPSTREAM_PULL, num), '-m', 'Prepare base for {}'.format(num)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    base_id = get_git(['log', '-1', '--format=%H', 'HEAD'])
    for i, pull_other in enumerate(pulls_mergeable):
        if num == pull_other.number:
            continue
        call_git(['checkout', base_id, '--quiet'])
        try:
            call_git(['merge', '{}/{}/head'.format(UPSTREAM_PULL, pull_other.number), '-m', 'Merge base_{}+{}'.format(num, pull_other.number), '--quiet'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except subprocess.CalledProcessError:
            call_git(['merge', '--abort'])
            conflicts += [pull_other]
    return conflicts


def update_comment(dry_run, pull, pulls_conflict):
    ID_CONFLICTS_COMMENT = '<!--e57a25ab6845829454e8d69fc972939a-->'

    if not pulls_conflict:
        for c in pull.get_issue_comments():
            if c.body.startswith(ID_CONFLICTS_COMMENT):
                # Empty existing comment
                text = ID_CONFLICTS_COMMENT
                text += 'No more conflicts as of last run.'
                if c.body == text:
                    return
                print('{}\n    .{}\n        .body = {}\n'.format(pull, c, text))
                if not dry_run:
                    c.edit(text)
                    return
        return

    text = ID_CONFLICTS_COMMENT
    text += 'Reviewers, this pull request conflicts with the following ones:\n'
    text += ''.join(['\n* [#{}](https://drahtbot.github.io/bitcoin_core_issue_redirect/r/{}.html) ({} by {})'.format(p.number, p.number, p.title.strip(), p.user.login) for p in pulls_conflict])
    text += '\n\n'
    text += 'If you consider this pull request important, please also help to review the conflicting pull requests. '
    text += 'Ideally, start with the one that should be merged first.'

    for c in pull.get_issue_comments():
        if c.body == text:
            # A comment is already up-to-date
            return
        if c.body.startswith(ID_CONFLICTS_COMMENT):
            # Our comment needs update
            print('{}\n    .{}\n        .body = {}\n'.format(pull, c, text))
            if not dry_run:
                c.edit(text)
            return

    # Couldn't find any comment
    print('{}\n    .new_comment.body = {}\n'.format(pull, text))
    if not dry_run:
        pull.create_issue_comment(text)


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Determine conflicting pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--pull_id', type=int, help='Update the conflict comment and label for this pull request.', default=0)
    parser.add_argument('--update_comments', action='store_true', help='Update all conflicts comments and labels.', default=False)
    parser.add_argument('--git_repo', help='The locally cloned git repo used for scratching', default=os.path.join(THIS_FILE_PATH, '..', 'scratch_conflicts'))
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    temp_dir = os.path.join(args.git_repo, '')
    os.makedirs(args.git_repo, exist_ok=True)
    args.git_repo = os.path.join(temp_dir, 'bitcoin')

    url = 'https://github.com/{}'.format(args.github_repo)
    if not os.path.isdir(args.git_repo):
        print('Clone {} repo to {}/bitcoin'.format(url, temp_dir))
        os.chdir(temp_dir)
        call_git(['clone', '--quiet', url, 'bitcoin'])
        print('Set git metadata')
        os.chdir(args.git_repo)
        with open(os.path.join(args.git_repo, '.git', 'config'), 'a') as f:
            f.write('[remote "{}"]\n'.format(UPSTREAM_PULL))
            f.write('    url = {}\n'.format(url))
            f.write('    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n')
            f.flush()
        call_git(['config', 'user.email', 'no@ne.nl'])
        call_git(['config', 'user.name', 'none'])

    print('Fetching diffs ...')
    os.chdir(args.git_repo)
    call_git(['gc'])
    call_git(['prune'])
    call_git(['fetch', '--quiet', '--all'])

    print('Fetching open pulls ...')
    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])
    call_git(['fetch', '--quiet', '--all'])  # Do it again just to be safe
    call_git(['fetch', 'origin', '{}'.format(args.base_name), '--quiet'])
    pulls = [p for p in pulls if p.base.ref == args.base_name]

    print('Open {}-pulls: {}'.format(args.base_name, len(pulls)))
    pulls_mergeable = [p for p in pulls if p.mergeable]
    print('Open mergeable {}-pulls: {}'.format(args.base_name, len(pulls_mergeable)))

    if args.update_comments:
        for i, pull_update in enumerate(pulls_mergeable):
            print('{}/{} Checking for conflicts {} <> {} <> {} ... '.format(i, len(pulls_mergeable), args.base_name, pull_update.number, 'other_pulls'))
            pulls_conflict = calc_conflicts(pulls_mergeable=pulls_mergeable, num=pull_update.number, base_branch=args.base_name)
            update_comment(dry_run=args.dry_run, pull=pull_update, pulls_conflict=pulls_conflict)

    if args.pull_id:
        pull_merge = [p for p in pulls if p.number == args.pull_id]

        if not pull_merge:
            print('{} not found in all {} open {} pulls'.format(args.pull_id, len(pulls), args.base_name))
            sys.exit(-1)
        pull_merge = pull_merge[0]

        if not pull_merge.mergeable:
            print('{} is not mergeable'.format(pull_merge.number))
            sys.exit(-1)

        print('Checking for conflicts {} <> {} <> {} ... '.format(args.base_name, pull_merge.number, 'other_pulls'))
        conflicts = calc_conflicts(pulls_mergeable=pulls_mergeable, num=pull_merge.number, base_branch=args.base_name)

        update_comment(dry_run=args.dry_run, pull=pull_merge, pulls_conflict=conflicts)


if __name__ == '__main__':
    main()
