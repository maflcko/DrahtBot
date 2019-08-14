import time
import shutil
import argparse
import os
import sys
import tempfile
import subprocess

from util.util import call_git, get_git


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Fetch depends and move them to /var/www/.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--git_ref', help='The git ref to checkout and fetch the depends from.', default='origin/master')
    parser.add_argument('--scratch_dir', help='The local scratch folder for temp downloads', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'depends_cache'))
    parser.add_argument('--dry_run', help='Print changes/edits instead of moving the files.', action='store_true', default=False)
    args = parser.parse_args()

    print()
    print('Same setup as the gitian builds:')
    print()
    print('sudo usermod -aG www-data $USER')
    print('sudo chown -R www-data:www-data /var/www')
    print('sudo chmod -R g+rw /var/www')
    print('mv /var/www/html/index.html /tmp/')
    print('# Then reboot')
    print()
    GIT_REMOTE_URL = 'https://github.com/{}'.format(args.github_repo)
    WWW_FOLDER_DEPENDS_CACHES = '/var/www/html/depends_download_fallback/'
    TEMP_DIR = os.path.join(args.scratch_dir, '')
    GIT_REPO_DIR = os.path.join(TEMP_DIR, 'git_repo')

    if not args.dry_run:
        print('Create folder {} if it does not exist'.format(WWW_FOLDER_DEPENDS_CACHES))
        os.makedirs(WWW_FOLDER_DEPENDS_CACHES, exist_ok=True)

    os.makedirs(TEMP_DIR, exist_ok=True)

    if not os.path.isdir(GIT_REPO_DIR):
        print('Clone {} repo to {}'.format(GIT_REMOTE_URL, GIT_REPO_DIR))
        os.chdir(TEMP_DIR)
        call_git(['clone', '--quiet', GIT_REMOTE_URL, GIT_REPO_DIR])
        print('Set git metadata')
        os.chdir(GIT_REPO_DIR)
        call_git(['config', 'user.email', 'no@ne.nl'])
        call_git(['config', 'user.name', 'none'])

    print('Fetch upsteam, checkout {}'.format(args.git_ref))
    os.chdir(GIT_REPO_DIR)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['checkout', args.git_ref])

    print('Download dependencies ...')
    os.chdir(os.path.join(GIT_REPO_DIR, 'depends'))
    os.environ['RAPIDCHECK'] = "1"
    subprocess.check_call(['make', 'download'])
    source_dir = os.path.join(GIT_REPO_DIR, 'depends', 'sources')
    print('Merging results of {} to {}'.format(source_dir, WWW_FOLDER_DEPENDS_CACHES))
    entries = [f.name for f in os.scandir(source_dir) if f.is_file()]
    for entry in entries:
        print(' ... entry = {}'.format(entry))
        if not args.dry_run:
            shutil.copyfile(src=os.path.join(source_dir, entry), dst=os.path.join(WWW_FOLDER_DEPENDS_CACHES, entry))


if __name__ == '__main__':
    main()
