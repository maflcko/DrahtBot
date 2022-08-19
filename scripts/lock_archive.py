from github import Github, GithubException, PullRequest
import argparse
import datetime

from util.util import return_with_pull_metadata

LOCK_REASON = 'resolved'

DAYS=365

def main():
    parser = argparse.ArgumentParser(description='Lock discussion on archived issues and pull requests.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='Comma separated list of repo slugs of the remotes on GitHub.', default='bitcoin-core/gui,bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    for github_repo in [github_api.get_repo(r) for r in args.github_repo.split(',')]:
        for getter in (github_repo.get_pulls, github_repo.get_issues):

            print(f'{getter.__name__} (closed) for repo {github_repo.owner.login}/{github_repo.name} ...')

            for el in getter(state='closed', direction='asc', sort='updated'):
                delta = datetime.datetime.utcnow() - el.updated_at
                print(f'Checking number #{el.number} with age {delta.days} days')
                if delta < datetime.timedelta(days=365):
                    print(f'All done up to {DAYS} days old')
                    break
                issue = el.as_issue() if type(el) is PullRequest.PullRequest else el
                if issue.locked:
                    print('Already locked')
                    continue
                print(f'{el}\n    .lock({LOCK_REASON})')
                if not args.dry_run:
                    issue.lock(LOCK_REASON)


if __name__ == '__main__':
    main()
