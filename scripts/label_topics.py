from github import Github, GithubException
import time
import argparse
import re
from collections import namedtuple

from util.util import return_with_pull_metadata

# Tuple of arrays of regexes
Needle = namedtuple('Needle', ['file', 'title'])

LABEL_NAME_TESTS = 'Tests'
LABEL_NAME_BACKPORT = 'Backport'

# Map from label name to Needle
LABELS = {
    'Build system': Needle(
        ['^configure', 'Makefile', '\.in$', '^depends', '^contrib/gitian'],
        ['^build:', '^depends:'],
    ),
    'TX fees and policy': Needle(
        ['^src/policy/'],
        ['^policy:'],
    ),
    'Utils/log/libs': Needle(
        ['^src/util/', '^src/crypto', '^src/key'],
        ['^log:'],
    ),
    'UTXO Db and Indexes': Needle(
        ['^src/txdb', '^src/index/', '^src/coins', '^src/leveldb', '^src/db'],
        [],
    ),
    'Validation': Needle(
        ['^src/validation', '^src/chain'],
        ['^validation:'],
    ),
    'interfaces': Needle(
        ['src/interfaces/'],
        ['^interfaces'],
    ),
    'Wallet': Needle(
        ['^src/wallet/', '^src/interfaces/wallet'],
        ['^wallet:'],
    ),
    'Consensus': Needle(
        ['^src/versionbits', '^src/script/(bitcoin|interpreter|script|sigcache)'],
        ['^consensus:'],
    ),
    'GUI': Needle(
        ['^src/qt'],
        ['^gui:', '^qt:'],
    ),
    'Mempool': Needle(
        ['^src/txmempool'],
        ['^mempool', '^txmempool:'],
    ),
    'Mining': Needle(
        ['^src/miner', '^src/rpc/mining'],
        ['^mining:'],
    ),
    'P2P': Needle(
        ['^src/net', '^src/tor', '^src/protocol'],
        ['^net:', '^p2p:'],
    ),
    'RPC/REST/ZMQ': Needle(
        ['^src/rpc', '^src/rest', '^src/zmq', '^src/wallet/rpc', '^src/http'],
        ['^rpc:', '^rest:', '^zmq:'],
    ),
    'Scripts and tools': Needle(
        ['^contrib/'],
        ['^contrib:'],
    ),
    LABEL_NAME_TESTS: Needle(
        ['^src/test', '^src/bench', '^src/qt/test', '^test', '^.appveyor', '^.cirrus', '^ci/', '^src/wallet/test', '^.travis'],
        ['^qa:', '^tests?:', '^ci:'],
    ),
    'Docs': Needle(
        ['^doc/', '.*.md$'],
        ['^docs?:'],
    ),
    LABEL_NAME_BACKPORT: Needle(
        [],
        ['^backport:'],
    ),
    'Refactoring': Needle(
        [],
        ['^refactor(ing)?:', '^move-?only:', '^scripted-diff:'],
    ),
}
LABELS = {l: Needle(
    [re.compile(r, flags=re.IGNORECASE) for r in LABELS[l].file],
    [re.compile(r, flags=re.IGNORECASE) for r in LABELS[l].title],
)
          for l in LABELS}


def main():
    parser = argparse.ArgumentParser(description='Update the pull request with missing labels.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    args = parser.parse_args()

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    print('Get labels ...')
    {l: github_repo.get_label(l) for l in LABELS}

    print('Get open pulls ...')
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open')])

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        issue = p.as_issue()
        new_labels = []
        if not len([l for l in issue.get_labels()]):
            if p.base.ref != github_repo.default_branch:
                new_labels = [LABEL_NAME_BACKPORT]  # Backports don't get topic labels
            else:
                modified_files = [f.filename for f in p.get_files()]
                print('{}: {}'.format(p.title, ', '.join(modified_files)))
                match = False
                for l in LABELS:
                    # Maybe this label matches the file
                    for f in modified_files:
                        for r in LABELS[l].file:
                            match = r.search(f)
                            if match:
                                break  # No need to check other regexes
                        if match:
                            break  # No need to check other files
                    if not match:  # Maybe this label matches the title
                        for r in LABELS[l].title:
                            match = r.search(issue.title)
                            if match:
                                break  # No need to check other regexes
                    if match:
                        if l == LABEL_NAME_TESTS and new_labels:
                            pass  # Avoid test label if there are already other labels
                        else:
                            new_labels += [l]
                        match = False
        if not new_labels:
            continue
        print('{}\n    .add_to_labels({})'.format(p, ', '.join(new_labels)))
        if not args.dry_run:
            issue.add_to_labels(*set(new_labels))


if __name__ == '__main__':
    main()
