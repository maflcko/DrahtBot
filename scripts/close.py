from github import Github, GithubException
from github3 import login

import time
import argparse
import datetime

from util.util import return_with_pull_metadata, IdComment

ID_CLOSED_COMMENT = IdComment.CLOSED.value


def main():
    parser = argparse.ArgumentParser(description='Close pull requests that need a rebase for too long.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_api_3 = login(token=args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)
    repo_owner, repo_name = args.github_repo.split('/')
    github_repo_3 = github_api_3.repository(repo_owner, repo_name)

    label_needs_rebase = github_repo.get_label('Needs rebase')

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        if not p.mergeable:
            issue = p.as_issue()
            if label_needs_rebase not in issue.get_labels():
                continue  # Should rarely happen

            delta = datetime.datetime.utcnow() - p.updated_at
            if delta < datetime.timedelta(days=30 * 9):
                continue
            text = ID_CLOSED_COMMENT
            text += "There hasn't been much activity lately and the patch still needs rebase, so I am closing this for now. Please let me know when you want to continue working on this, so the pull request can be re-opened."
            print('{}\n    .close()'.format(p))
            print('    .create_comment({})'.format(text))
            if not args.dry_run:
                issue.create_comment(text)
                pull_3 = github_repo_3.pull_request(p.number)
                assert pull_3.close()
            continue


if __name__ == '__main__':
    main()
