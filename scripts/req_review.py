from github import Github, GithubException
import argparse
import fnmatch
import re

from util.util import return_with_pull_metadata, IdComment

ID_REVIEWERS_REQUESTED_COMMENT = IdComment.REVIEWERS_REQUESTED.value
FILENAME_REVIEWERS = 'REVIEWERS'
BASE_BRANCH = 'master'


def return_reviewers(reviewers_file):
    reviewers_file_content = reviewers_file.splitlines()
    file_match_to_reviewer = {}  # format {'doc/Doxyfile.in': '@fanquake( @more @...)'}
    for line in reviewers_file_content:
        if line and not line.startswith("#"):  # ignores commented or empty lines
            file_to_reviewer_group = re.match(r"(^[^\s]*)\s+(@.*)", line).groups()
            file_match_to_reviewer.update({file_to_reviewer_group[0]: file_to_reviewer_group[1]})  # strip off / rather than just the first char
    return file_match_to_reviewer


def reviewers_for_files(file_match_to_reviewer, files_changed):
    reviewers = set()
    for f in files_changed:
        for match, owners in file_match_to_reviewer.items():
            # strip off leading "/" for match (files provided by GitHub don't have leading "/")
            if match.startswith("/"):
                match = match[1:]
            if fnmatch.fnmatch(f, match):  # match for file system
                list_of_owners = owners.split(" ")  # break multiple usernames into a list
                for username in list_of_owners:
                    reviewers.add(username)  # add to set to preserve uniqueness
    return reviewers


def main():
    parser = argparse.ArgumentParser(description='Request reviewers.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='Comma separated list of repo slugs of the remotes on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    for github_repo in [github_api.get_repo(r) for r in args.github_repo.split(',')]:
        reviewers_file = github_repo.get_contents(FILENAME_REVIEWERS).decoded_content.decode("utf-8")
        file_match_to_reviewer = return_reviewers(reviewers_file)

        print(f'Get open pulls for repo {github_repo.owner.login}/{github_repo.name} {BASE_BRANCH} branch ...')
        pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open', base=BASE_BRANCH)])

        print(f'Open {BASE_BRANCH} pulls: {len(pulls)}')

        for i, p in enumerate(pulls):
            print('{}/{}'.format(i, len(pulls)))
            if p.mergeable_state == 'draft':
                # Exclude draft pull requests
                continue
            if p.number < 20653:
                # Exclude old pull requests
                continue
            issue = p.as_issue()
            comments = p.get_issue_comments()
            # check for review request comment already posted
            review_comment = [c for c in comments if c.body.startswith(ID_REVIEWERS_REQUESTED_COMMENT)]
            if review_comment:
                print(f'Reviewers already requested for {p}')
                continue
            discard_set = [issue.user.login]  # remove author from set if included
            discard_set += [c.user.login for c in comments]
            discard_set += [c.user.login for c in p.get_review_comments()]
            # check for match from REVIEWERS file and add comment for reviewers
            pull_files = [p.filename for p in p.get_files()]
            requested_reviewers = reviewers_for_files(file_match_to_reviewer, pull_files)
            for discard in discard_set:
                requested_reviewers.discard('@' + discard)
            if requested_reviewers:
                review_request_text = ID_REVIEWERS_REQUESTED_COMMENT + "\n🕵️ "
                for reviewer in requested_reviewers:
                    review_request_text += reviewer + ' '
                if len(requested_reviewers) == 1:
                    review_request_text += 'has been '
                else:
                    review_request_text += 'have been '
                review_request_text += f'requested to review this pull request as specified in the {FILENAME_REVIEWERS} file.'
                print(f'{p}\n    .create_comment({review_request_text})')
                if not args.dry_run:
                    issue.create_comment(review_request_text)
                continue


if __name__ == '__main__':
    main()
