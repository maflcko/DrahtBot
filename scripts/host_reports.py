import argparse
import os

from util.util import call_git


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Pull a git repository and move it to /var/www/... .', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--repo_report', help='The repo slug of the remote on GitHub for reports.', default='DrahtBot/reports')
    parser.add_argument('--host_reports_folder', help='The local scratch folder', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'host_reports'))
    parser.add_argument('--dry_run', help='Print changes/edits, only modify the scratch folder.', action='store_true', default=False)
    args = parser.parse_args()

    print()
    print('See guix.py or gitian.py for instructions on how to add write permission for /var/www to the current user')
    print()
    repo_url = 'https://github.com/{}'.format(args.repo_report)
    host_reports_www_folder = '/var/www/html/host_reports/{}/'.format(args.repo_report)
    temp_dir = os.path.normpath(os.path.join(args.host_reports_folder, ''))

    if args.dry_run:
        host_reports_www_folder = os.path.join(temp_dir, 'www_output/')

    if not os.path.isdir(host_reports_www_folder):
        print('Clone {} repo to {}'.format(repo_url, host_reports_www_folder))
        call_git(['clone', '--quiet', repo_url, host_reports_www_folder])

    print('Fetch upsteam, checkout latest `main` branch')
    os.chdir(host_reports_www_folder)
    call_git(['fetch', '--quiet', '--all'])
    call_git(['checkout', 'origin/main'])
    call_git(['reset', '--hard', 'HEAD'])


if __name__ == '__main__':
    main()
