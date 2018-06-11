from github import Github
import time
import argparse


def main():
    parser = argparse.ArgumentParser(description='Update the label that indicates a rebase is required.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_rebase = github_repo.get_label('Needs rebase')

    while True:
        print('Get open pulls ...')
        pulls = [p for p in github_repo.get_pulls(state='open')]
        print('Fetching open pulls metadata ...')
        pulls_update_mergeable = lambda: [p for p in pulls if p.mergeable is None]
        while pulls_update_mergeable():
            print('Update mergable state for pulls {}'.format([p.number for p in pulls_update_mergeable()]))
            [p.update() for p in pulls_update_mergeable()]

        print('Open pulls: {}'.format(len(pulls)))

        for i, p in enumerate(pulls):
            print('{}/{}'.format(i, len(pulls)))
            issue = p.as_issue()
            if p.mergeable and label_needs_rebase in issue.get_labels():
                print('{}.remove_from_labels({})'.format(p, label_needs_rebase))
                if not args.dry_run:
                    issue.remove_from_labels(label_needs_rebase)
                continue
            if not p.mergeable and label_needs_rebase not in issue.get_labels():
                print('{}.add_to_labels({})'.format(p, label_needs_rebase))
                if not args.dry_run:
                    issue.add_to_labels(label_needs_rebase)
                    ID_NEEDS_REBASE_COMMENT = '<!--cf906140f33d8803c4a75a2196329ecb-->'
                    issue.create_comment(ID_NEEDS_REBASE_COMMENT + 'Needs rebase')
        PAUSE = 1 * 60 * 60
        print('Sleeping for {}s'.format(PAUSE))
        time.sleep(PAUSE)


if __name__ == '__main__':
    main()
