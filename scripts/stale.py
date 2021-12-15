from github import Github

import time
import argparse
import datetime

from util.util import return_with_pull_metadata, IdComment

ID_STALE_COMMENT = IdComment.STALE.value


def main():
    parser = argparse.ArgumentParser(description='Comment on pull requests that needed a rebase for too long.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repos', help='The comma-separated repo slugs of the remotes on GitHub.', default='bitcoin-core/gui,bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    for slug in args.github_repos.split(','):
        github_repo = github_api.get_repo(slug)

        label_needs_rebase = github_repo.get_label('Needs rebase')
        label_up_for_grabs = github_repo.get_label('Up for grabs')

        print(f'Get open pulls for {slug} ...')
        pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

        print(f'Open pulls for {slug}: {len(pulls)}')

        for i, p in enumerate(pulls):
            print('{}/{}'.format(i, len(pulls)))
            if not p.mergeable:
                issue = p.as_issue()
                if label_needs_rebase not in issue.get_labels():
                    continue  # Should rarely happen

                delta = datetime.datetime.utcnow() - p.updated_at
                if delta < datetime.timedelta(days=30 * 3):
                    continue  # Too recent
                text = ID_STALE_COMMENT
                text += "There hasn't been much activity lately and the patch still needs rebase. What is the status here?\n"
                text += "\n"
                text += "* Is it still relevant? ➡️ Please solve the conflicts to make it ready for review and to ensure the CI passes.\n"
                text += "* Is it no longer relevant? ➡️ Please close.\n"
                text += "* Did the author lose interest or time to work on this? ➡️ Please close it and mark it 'Up for grabs' with the label, so that it can be picked up in the future.\n"
                print(f'{p}\n    .create_comment({text})')
                if not args.dry_run:
                    issue.create_comment(text)
                continue


if __name__ == '__main__':
    main()
