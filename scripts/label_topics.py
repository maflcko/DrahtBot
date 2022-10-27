from github import Github, GithubException
import time
import argparse
import re
from collections import namedtuple

# Tuple of arrays of regexes
Needle = namedtuple('Needle', ['title'])

LABEL_NAME_DOCS = 'Docs'
LABEL_NAME_TESTS = 'Tests'
LABEL_NAME_BACKPORT = 'Backport'
LABEL_NAME_REFACTORING = 'Refactoring'

# Map from label name to Needle
LABELS = {
    'Build system': Needle(
        ['^guix:', '^build:', '^depends:'],
    ),
    'TX fees and policy': Needle(
        ['^policy:'],
    ),
    'Utils/log/libs': Needle(
        ['^log:', '^util:', '^crypto:', '^libs:', '^compat:'],
    ),
    'UTXO Db and Indexes': Needle(
        ['^index:', '^indexes:', '^txdb:', '^coins:' , '^db:'],
    ),
    'Block storage': Needle(
        ['^blockstorage:'],
    ),
    'PSBT': Needle(
        ['^psbt:'],
    ),
    'Validation': Needle(
        ['^validation:', '^chain:', '^kernel:'],
    ),
    'interfaces': Needle(
        ['^interfaces:'],
    ),
    'Wallet': Needle(
        ['^wallet:'],
    ),
    'Descriptors': Needle(
        ['^descriptors:', '^miniscript:'],
    ),
    'Consensus': Needle(
        ['^consensus:', '^versionbits:', '^interpreter:', '^script:', '^sigcache:'],
    ),
    'GUI': Needle(
        ['^gui:', '^qt:'],
    ),
    'Mempool': Needle(
        ['^mempool', '^txmempool:'],
    ),
    'Mining': Needle(
        ['^mining:', '^miner:'],
    ),
    'P2P': Needle(
        ['^net:', '^p2p:', '^tor:', '^addrman:', '^protocol:', '^net processing:'],
    ),
    'RPC/REST/ZMQ': Needle(
        ['^univalue:', '^rpc:', '^rest:', '^zmq:', '^http:'],
    ),
    'Scripts and tools': Needle(
        ['^contrib:'],
    ),
    LABEL_NAME_TESTS: Needle(
        ['^qa:', '^fuzz:', '^tests?:', '^ci:', '^bench:', '^cirrus:'],
    ),
    LABEL_NAME_DOCS: Needle(
        ['^docs?:'],
    ),
    LABEL_NAME_BACKPORT: Needle(
        ['^backport:'],
    ),
    LABEL_NAME_REFACTORING: Needle(
        ['^refactor(ing)?:', '^move-?only:', '^scripted-diff:'],
    ),
}
LABELS = {l: Needle(
    [re.compile(r, flags=re.IGNORECASE) for r in LABELS[l].title],
)
          for l in LABELS}


def MaybeClean(labels):
    labels_set = set(labels)
    if len(labels_set) >= 4:
        return {LABEL_NAME_REFACTORING}
    labels_secondary = {LABEL_NAME_TESTS, LABEL_NAME_DOCS}
    labels_primary = labels_set - labels_secondary
    if labels_primary:
        return labels_primary
    return labels


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
    pulls = [p for p in github_repo.get_pulls(state='open')]

    print('Open pulls: {}'.format(len(pulls)))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))
        issue = p.as_issue()
        new_labels = []
        if not len([l for l in issue.get_labels()]):
            if p.base.ref != github_repo.default_branch:
                new_labels = [LABEL_NAME_BACKPORT]  # Backports don't get topic labels
            if not new_labels:
                for l in LABELS:
                    if any(r.search(issue.title) for r in LABELS[l].title):
                        new_labels = [l]
                        break  # no need to check other labels
        if not new_labels:
            continue
        new_labels = MaybeClean(new_labels)
        print('{}\n    .add_to_labels({})'.format(p, ', '.join(new_labels)))
        if not args.dry_run:
            issue.add_to_labels(*set(new_labels))


if __name__ == '__main__':
    main()
