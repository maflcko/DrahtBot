import time
import argparse

from github import Github, GithubException
from github3 import login

from util.util import return_with_pull_metadata, IdComment


def main():
    parser = argparse.ArgumentParser(description='Update the label that indicates a rebase is required.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_api_3 = login(token=args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)
    repo_owner, repo_name = args.github_repo.split('/')
    github_repo_3 = github_api_3.repository(repo_owner, repo_name)

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open', sort='created')[:5]])

    print('To check pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        issue = p.as_issue()
        for c in issue.get_comments():
            if c.body.startswith('Pull requests without a rationale and clear improvement may be closed'):
                print('{}\n    .close()'.format(p, len(comments)))
                if not args.dry_run:
                    pull_3 = github_repo_3.pull_request(p.number)
                    assert pull_3.close()
            break  # Only check first comment


if __name__ == '__main__':
    main()
