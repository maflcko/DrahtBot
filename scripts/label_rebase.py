from github import Github, GithubException
import time
import argparse

from util.util import return_with_pull_metadata, IdComment

ID_NEEDS_REBASE_COMMENT = IdComment.NEEDS_REBASE.value
AVOID_COMMENT_ISSUES = [10973, 10102]


def main():
    parser = argparse.ArgumentParser(description='Update the label that indicates a rebase is required.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_rebase = github_repo.get_label('Needs rebase')

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        if p.mergeable_state == 'draft':
            # Exclude draft pull requests
            continue
        issue = p.as_issue()
        if p.mergeable and label_needs_rebase in issue.get_labels():
            print('{}\n    .remove_from_labels({})'.format(p, label_needs_rebase))
            comments = [c for c in issue.get_comments() if c.body.startswith(ID_NEEDS_REBASE_COMMENT)]
            print('    + delete {} comments'.format(len(comments)))
            if not args.dry_run:
                issue.remove_from_labels(label_needs_rebase)
                for c in comments:
                    c.delete()
            continue
        if not p.mergeable and label_needs_rebase not in issue.get_labels():
            print('{}\n    .add_to_labels({})'.format(p, label_needs_rebase))
            if not args.dry_run:
                issue.add_to_labels(label_needs_rebase)
                if issue.number not in AVOID_COMMENT_ISSUES:
                    text = ID_NEEDS_REBASE_COMMENT
                    text += '\n🐙 This pull request conflicts with the target branch and [needs rebase](https://github.com/bitcoin/bitcoin/blob/fa733bbd78add587e19f0175ab9c127a8c27e024/CONTRIBUTING.md#rebasing-changes).'
                    issue.create_comment(text)
            continue


if __name__ == '__main__':
    main()
