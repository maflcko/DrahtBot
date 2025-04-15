#  install
#  -------
#
#  ```
#  virtualenv --python=python3 ./env_3
#  source ./env_3/bin/activate
#  pip install mwclient
#  ```

import mwclient
import argparse
import os
import time
import glob
import subprocess

def call_git(args, **kwargs):
    subprocess.check_call(['git'] + args, **kwargs)

def get_git(args):
    return subprocess.check_output(['git'] + args, universal_newlines=True).strip()

def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Update the BIPs on the wiki with the latest text from the git repo.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bips')
    parser.add_argument('--mediawiki_login_tuple', help='The login tuple for the mediawiki.', default='None:None')
    parser.add_argument('--mediawiki_host', help='The mediawiki host.', default='en.bitcoin.it')
    parser.add_argument('--scratch_dir', help='The local dir used for scratching', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'bips_mediawiki'))
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the MediaWiki API.', action='store_true', default=False)
    args = parser.parse_args()

    site = mwclient.Site(host=args.mediawiki_host)
    if not args.dry_run:
        login_tuple = args.mediawiki_login_tuple.split(':', 1)
        site.login(login_tuple[0], login_tuple[1])

    args.scratch_dir = os.path.abspath(os.path.join(args.scratch_dir, ''))
    os.makedirs(args.scratch_dir, exist_ok=True)

    code_dir = os.path.join(args.scratch_dir, 'bips_git', args.github_repo)
    code_url = 'https://github.com/{}'.format(args.github_repo)

    def create_scratch_dir(folder, url):
        if os.path.isdir(folder):
            return
        print('Clone {} repo to {}'.format(url, folder))
        os.chdir(args.scratch_dir)
        call_git(['clone', '--quiet', url, folder])

    create_scratch_dir(code_dir, code_url)

    print('Fetching diffs ...')
    os.chdir(code_dir)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['reset', '--hard', 'HEAD'])
    call_git(['checkout', 'master'])
    call_git(['pull', '--ff-only', 'origin', 'master'])

    commit_id = get_git(['log', '-1', '--format=%H'])[:16]
    for file_name in glob.glob('bip-*.mediawiki'):
        bip_number = int(file_name.split('bip-', 1)[1].split('.mediawiki')[0])
        print('Reading BIP {:04d} ...'.format(bip_number))
        with open(file_name, encoding='utf-8') as f:
            content = f.read()
        page = site.pages['BIP {:04d}'.format(bip_number)]
        edit_summary = 'Update BIP text with latest version from {}/blob/{}/{}'.format(code_url, commit_id, file_name)
        print(edit_summary)
        if not args.dry_run:
            page.save('{{bip}}\n' + '{{BipMoved|' + file_name + '}}\n\n' + content, edit_summary)
            time.sleep(5)
            site.pages['bip-{:04d}.mediawiki'.format(bip_number)].save(
                '#REDIRECT [[BIP {:04d}]]'.format(bip_number),
                'Create redirect from [[bip-{:04d}.mediawiki]] to [[BIP {:04d}]]'.format(bip_number, bip_number),
            )
            time.sleep(5)


if __name__ == '__main__':
    main()
