from github import Github, GithubException
import time
import argparse
import re

from util.util import return_with_pull_metadata

# Map from label name to regex modified
LABELS = {
    'Build system': ['^configure', 'Makefile', '\.in$', '^depends', '^contrib/gitian'],
    'TX fees and policy': ['^src/policy/'],
    'Utils/log/libs': ['^src/util/', '^src/crypto', '^src/key'],
    'UTXO Db and Indexes': ['^src/txdb', '^src/index/', '^src/coins', '^src/leveldb', '^src/db'],
    'Validation': ['^src/validation', '^src/chain'],
    'Wallet': ['^src/wallet/', '^src/interfaces/wallet'],
    'Consensus': ['^src/versionbits', '^src/script/(bitcoin|interpreter|script|sigcache)'],
    'GUI': ['^src/qt'],
    'Mempool': ['^src/txmempool'],
    'Mining': ['^src/miner', '^src/rpc/mininig'],
    'P2P': ['^src/net', '^src/tor'],
    'RPC/REST/ZMQ': ['^src/rpc', '^src/rest', '^src/zmq', '^src/wallet/rpc', '^src/http'],
    'Scripts and tools': ['^contrib/'],
    'Tests': ['^src/test', '^src/qt/test', '^test', '^src/wallet/test'],
    'Docs': ['^doc/', '.*.md$'],
}
LABELS = {l: [re.compile(r) for r in LABELS[l]] for l in LABELS}


def main():
    parser = argparse.ArgumentParser(description='Update the pull request with missing labels.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    print('Get labels ...')
    labels = {l: github_repo.get_label(l) for l in LABELS}

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        issue = p.as_issue()
        if not len([l for l in issue.get_labels()]):
            modified_files = [f.filename for f in p.get_files()]
            print('{}: {}'.format(p.title, ', '.join(modified_files)))
            new_labels = []
            match = False
            for l in LABELS:
                for f in modified_files:
                    for r in LABELS[l]:
                        match = r.search(f)
                        if match:
                            break  # No need to check other regexes
                    if match:
                        break  # No need to check other files
                if match:
                    new_labels += [labels[l]]
                    match = False
            print('{}\n    .add_to_labels({})'.format(p, ', '.join([l.name for l in new_labels])))
            if not args.dry_run:
                issue.add_to_labels(label_needs_rebase)


if __name__ == '__main__':
    main()
