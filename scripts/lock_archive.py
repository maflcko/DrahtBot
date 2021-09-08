from github import Github, GithubException
import argparse

from util.util import return_with_pull_metadata

LOCK_REASON = 'resolved'


def main():
    parser = argparse.ArgumentParser(description='Lock discussion on archived issues and pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='Comma separated list of repo slugs of the remotes on GitHub.', default='bitcoin-core/gui,bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    parser.add_argument('--year', help='Archive all pull requests from this year (and previous years).', type=int, default=2019)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    for github_repo in [github_api.get_repo(r) for r in args.github_repo.split(',')]:

        print(f'Get closed pulls for repo {github_repo.owner.login}/{github_repo.name} ...')

        for i, p in enumerate(github_repo.get_pulls(state='closed', direction='asc', sort='created')):
            print(f'Checking pull number #{p.number} from year {p.updated_at.year} against {args.year}')
            if p.updated_at.year > args.year:
                # Too recent
                continue
            issue = p.as_issue()
            if issue.locked:
                continue
            print(f'{p}\n    .lock({LOCK_REASON})')
            if not args.dry_run:
                issue.lock(LOCK_REASON)


if __name__ == '__main__':
    main()
