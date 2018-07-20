from github import Github, GithubException
import time
import urllib.request
import re
import datetime
import argparse

from travispy import TravisPy
from github3 import login

from util.util import return_with_pull_metadata

ID_TRAVIS_RE_COMMENT = '<!--5d09a71f8925f3f132321140b44b946d-->'


def main():
    parser = argparse.ArgumentParser(description='Trigger a travis run if the current one is too old.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_api_3 = login(token=args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)
    repo_owner, repo_name = args.github_repo.split('/')
    github_repo_3 = github_api_3.repository(repo_owner, repo_name)
    travis_api = TravisPy()

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        if p.mergeable:
            html_source_commits = urllib.request.urlopen('https://github.com/{}/pull/{}/commits'.format(args.github_repo, p.number)).read().decode()
            build_finish = datetime.datetime.min
            for build_id in html_source_commits.split('https://travis-ci.org/bitcoin/bitcoin/builds/')[1:]:
                build_id = int(re.search('\d+', build_id).group(0))
                build_finished = travis_api.build(build_id).finished_at
                if not build_finished:
                    # Try next build
                    continue
                parsed = datetime.datetime.strptime(build_finished, "%Y-%m-%dT%H:%M:%SZ")
                if build_finish < parsed:
                    build_finish = parsed
            if build_finish == datetime.datetime.min:
                # No travis result in any build or no builds
                continue
            delta = datetime.datetime.utcnow() - build_finish
            if delta < datetime.timedelta(days=400):
                continue

            issue = p.as_issue()
            comments = [c for c in issue.get_comments() if c.body.startswith(ID_TRAVIS_RE_COMMENT)]
            text = ID_TRAVIS_RE_COMMENT
            text += 'The last travis run for this pull request was {} days ago. To trigger a fresh travis build, this pull request should be closed and re-opened.'.format(delta.days)
            print('{}\n    .delete {} comments'.format(p, len(comments)))
            print('    .open()')
            print('    .create_comment({})'.format(text))
            if not args.dry_run:
                pull_3 = github_repo_3.pull_request(p.number)
                assert pull_3.close()
                for c in comments:
                    c.delete()
                issue.create_comment(text)
                assert pull_3.open()
            continue


if __name__ == '__main__':
    main()
