from github import Github, GithubException
import time
import itertools
from collections import defaultdict
import shutil
import argparse
import os
import sys
import tempfile
import subprocess

from util.util import return_with_pull_metadata, call_git, get_git

ID_GUIX_COMMENT = '<!--9cd9c72976c961c55c7acef8f6ba82cd-->'
UPSTREAM_PULL = 'upstream-pull'


def calculate_diffs(folder_1, folder_2):
    EXTENSIONS = ['.log']
    files = set(os.listdir(folder_1)).intersection(set(os.listdir(folder_2)))
    files = [f for f in files if any(f.endswith(e) for e in EXTENSIONS)]
    for f in files:
        os.chdir(folder_2)
        file_1 = str(os.path.join(folder_1, f))
        file_2 = str(os.path.join(folder_2, f))
        subprocess.call('diff --color {} {} > {}.diff'.format(file_1, file_2, f), shell=True)


def main():
    THIS_FILE_PATH = os.path.abspath(os.path.dirname(os.path.realpath(__file__)))
    parser = argparse.ArgumentParser(description='Guix build and create an issue comment to share the results.', formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument('--github_access_token', help='The access token for GitHub.', default='')
    parser.add_argument('--github_repo', help='The repo slug of the remote on GitHub.', default='bitcoin/bitcoin')
    parser.add_argument('--base_name', help='The name of the base branch.', default='master')
    parser.add_argument('--guix_folder', help='The local scratch folder for temp guix results', default=os.path.join(THIS_FILE_PATH, '..', 'scratch', 'guix'))
    parser.add_argument('--guix_jobs', help='The number of jobs', default=2)
    parser.add_argument('--domain', help='Where the assets are reachable', default='http://127.0.0.1')
    parser.add_argument('--dry_run', help='Print changes/edits instead of calling the GitHub API.', action='store_true', default=False)
    parser.add_argument('--build_one_commit', help='Only build this one commit and exit.', default='')
    args = parser.parse_args()

    print()
    print('Make sure to install docker and run the https://docs.docker.com/install/linux/linux-postinstall/')
    print('sudo usermod -aG docker $USER')
    print()
    print('sudo usermod -aG www-data $USER')
    print('sudo chown -R www-data:www-data /var/www')
    print('sudo chmod -R g+rw /var/www')
    print('mv /var/www/html/index.html /tmp/')
    print('# Then reboot')
    print()
    url = 'https://github.com/{}'.format(args.github_repo)
    guix_www_folder = '/var/www/html/guix/{}/'.format(args.github_repo)
    external_url = '{}/guix/{}/'.format(args.domain, args.github_repo)

    if not args.dry_run:
        print('Clean guix folder of old files')
        subprocess.check_call('find {} -mindepth 1 -maxdepth 1 -type d -ctime +{} | xargs rm -rf'.format(guix_www_folder, 15), shell=True)
        os.makedirs(guix_www_folder, exist_ok=True)

    temp_dir = os.path.normpath(os.path.join(args.guix_folder, ''))
    os.makedirs(temp_dir, exist_ok=True)
    git_repo_dir = os.path.join(temp_dir, args.github_repo)
    depends_sources_dir = os.path.join(temp_dir, 'depends_sources')
    depends_cache_dir = os.path.join(temp_dir, 'depends_cache')
    guix_store_dir = os.path.join(temp_dir, 'root_store')
    guix_bin_dir = os.path.join(temp_dir, 'root_bin')
    os.makedirs(depends_sources_dir, exist_ok=True)
    os.makedirs(depends_cache_dir, exist_ok=True)
    os.makedirs(guix_store_dir, exist_ok=True)
    os.makedirs(guix_bin_dir, exist_ok=True)

    if not os.path.isdir(git_repo_dir):
        print('Clone {} repo to {}'.format(url, git_repo_dir))
        os.chdir(temp_dir)
        call_git(['clone', '--quiet', url, git_repo_dir])
        print('Set git metadata')
        os.chdir(git_repo_dir)
        with open(os.path.join(git_repo_dir, '.git', 'config'), 'a') as f:
            f.write('[remote "{}"]\n'.format(UPSTREAM_PULL))
            f.write('    url = {}\n'.format(url))
            f.write('    fetch = +refs/pull/*:refs/remotes/upstream-pull/*\n')
            f.flush()
        call_git(['config', 'user.email', 'no@ne.nl'])
        call_git(['config', 'user.name', 'none'])
    print('Fetch upsteam pulls')
    os.chdir(git_repo_dir)
    call_git(['fetch', '--quiet', '--all'])

    print('Start docker process ...')
    docker_id = subprocess.check_output(
        [
            'docker',
            'run',
            '-idt',
            '--rm',
            '--privileged',  # https://github.com/bitcoin/bitcoin/pull/17595#issuecomment-606407804
            '--volume={}:{}:rw,z'.format(guix_store_dir, '/gnu'),
            '--volume={}:{}:rw,z'.format(guix_bin_dir, '/var/guix'),
            '--volume={}:{}:rw,z'.format(temp_dir, temp_dir),
            #'--mount', # Doesn't work with fedora (needs rw,z)
            #'type=bind,src={},dst={}'.format(dir_code, dir_code),
            #'-e',
            #'LC_ALL=C.UTF-8',
            'ubuntu:focal',  
        ],
        universal_newlines=True,
    ).strip()

    print('Docker running with id {}.'.format(docker_id))
    docker_bash_prefix = ['true']
    docker_exec = lambda cmd: subprocess.check_call(['docker', 'exec', docker_id, 'bash', '-c', 'export TMPDIR=/guix_temp_dir/ && {} && cd {} && {}'.format(docker_bash_prefix[0], os.getcwd(), cmd)], universal_newlines=True)
    docker_exec('mkdir /guix_temp_dir/')

    print('Installing packages ...')
    docker_exec('apt-get update')
    docker_exec('apt-get install -qq {}'.format('netbase wget xz-utils git make curl'))

    os.chdir(temp_dir)
    if not os.listdir(guix_store_dir):
       print('Install guix')
       docker_exec('wget https://ftp.gnu.org/gnu/guix/guix-binary-1.1.0.x86_64-linux.tar.xz')
       docker_exec('echo "eae0b8b4ee8ba97e7505dbb85d61ab2ce7f0195b824d3a660076248d96cdaece  ./guix-binary-1.1.0.x86_64-linux.tar.xz" | sha256sum -c')
       docker_exec('tar -xf ./guix-binary-1.1.0.x86_64-linux.tar.xz')
       docker_exec('mv var/guix/* /var/guix && mv gnu/* /gnu/')

    docker_exec('mkdir -p /config_guix/')
    docker_exec('ls -lh /config_guix/')
    docker_exec('ln -sf /var/guix/profiles/per-user/root/current-guix /config_guix/current')
    docker_bash_prefix[0] = 'source /config_guix/current/etc/profile'
    docker_exec('groupadd --system guixbuild')
    docker_exec('for i in `seq -w 1 10`; do useradd -g guixbuild -G guixbuild -d /var/empty -s `which nologin` -c "Guix build user $i" --system guixbuilder$i; done')

    docker_exec('wget -qO- "https://guix.carldong.io/signing-key.pub" | guix archive --authorize')
    docker_exec('guix archive --authorize < /config_guix/current/share/guix/ci.guix.info.pub')

    def call_guix_build(*, commit):
        os.chdir(git_repo_dir)
        call_git(['clean', '-dfx'])
        call_git(['checkout', '--quiet', '--force', commit])
        depends_compiler_hash = get_git(['rev-parse','{}:./contrib/guix'.format(commit)])
        depends_cache_subdir = os.path.join(depends_cache_dir, depends_compiler_hash)
        docker_exec("cp -r {}/built {}/depends/".format(depends_cache_subdir, git_repo_dir))
        docker_exec("sed -i -e 's/--disable-bench //g' $(git grep -l disable-bench ./contrib/guix/)")
        docker_exec("sed -i -e 's/DISTSRC}\/doc\/README.md/DISTSRC}\/..\/doc\/README.md/g' ./contrib/guix/libexec/build.sh") # TEMPORARY
        docker_exec("( guix-daemon --build-users-group=guixbuild & ) && (export V=1 && export VERBOSE=1 && export MAX_JOBS={} && export SOURCES_PATH={} && ./contrib/guix/guix-build.sh > {}/outerr 2>&1 )".format(args.guix_jobs, depends_sources_dir, git_repo_dir))
        docker_exec("rm -rf {}/*".format(depends_cache_dir))
        os.makedirs(depends_cache_subdir, exist_ok=True)
        docker_exec("mv {}/depends/built {}/built".format(git_repo_dir, depends_cache_subdir))
        docker_exec("mv {}/outerr {}/output/guix_build.log".format(git_repo_dir, git_repo_dir))
        docker_exec("mv {}/output/src/* {}/output/".format(git_repo_dir, git_repo_dir))
        docker_exec("rmdir {}/output/src".format(git_repo_dir))
        return os.path.join(git_repo_dir, 'output')

    if args.build_one_commit:
        print('Starting guix build for one commit ({}) ...'.format(args.build_one_commit))
        output_dir = call_guix_build(commit=args.build_one_commit)
        print('See folder:\n{}'.format(output_dir))
        print('Exit')
        return

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_guix = github_repo.get_label('Needs guix build')

    print('Get open, mergeable {} pulls ...'.format(args.base_name))
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open', base=args.base_name)])
    os.chdir(git_repo_dir)
    call_git(['fetch', '--quiet', '--all'])  # Do it again just to be safe
    call_git(['fetch', '--quiet', 'origin'])
    base_commit = get_git(['log', '-1', '--format=%H', 'origin/{}'.format(args.base_name)])
    pulls = [p for p in pulls if p.mergeable]

    print('Num: {}'.format(len(pulls)))

    pulls = [p.as_issue() for p in pulls]
    pulls = [i for i in pulls if label_needs_guix in i.get_labels()]
    if not pulls:
        print('Nothing tagged with {}. Exiting...'.format(label_needs_guix.name))
        return

    print('Starting guix build for base branch ...')
    base_folder = call_guix_build(commit=base_commit)
    if not args.dry_run:
        print('Moving results of {} to {}'.format(base_folder, guix_www_folder))
        shutil.rmtree(os.path.join(guix_www_folder, base_commit), ignore_errors=True)
        base_folder = shutil.move(src=base_folder, dst=os.path.join(guix_www_folder, base_commit))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))

        print('Starting guix build ...')
        os.chdir(git_repo_dir)
        commit = get_git(['log', '-1', '--format=%H', '{}/{}/merge'.format(UPSTREAM_PULL, p.number)])
        commit_folder = call_guix_build(commit=commit)
        if not args.dry_run:
            print('Moving results of {} to {}'.format(commit, guix_www_folder))
            shutil.rmtree(os.path.join(guix_www_folder, commit), ignore_errors=True)
            commit_folder = shutil.move(src=commit_folder, dst=os.path.join(guix_www_folder, commit))

        calculate_diffs(base_folder, commit_folder)

        text = ID_GUIX_COMMENT
        text += '\n'
        text += '### Guix builds\n\n'
        text += '| File '
        text += '| commit {}<br>({}) '.format(base_commit, args.base_name)
        text += '| commit {}<br>({} and this pull) '.format(commit, args.base_name)
        text += '|\n'
        text += '|--|--|--|\n'

        text += calculate_table(base_folder, commit_folder, external_url, base_commit, commit)

        print('{}\n    .remove_from_labels({})'.format(p, label_needs_guix))
        print('    .create_comment({})'.format(text))

        if not args.dry_run:
            issue.create_comment(text)
            issue.remove_from_labels(label_needs_guix)


def calculate_table(base_folder, commit_folder, external_url, base_commit, commit):
    rows = defaultdict(lambda: ['', ''])  # map from file name to list of links
    for f in sorted(os.listdir(base_folder)):
        os.chdir(base_folder)
        left = rows[f]
        left[0] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, base_commit, f)
        rows[f] = left

    for f in sorted(os.listdir(commit_folder)):
        os.chdir(commit_folder)
        right = rows[f]
        right[1] = '[`{}...`]({}{}/{})'.format(subprocess.check_output(['sha256sum', f], universal_newlines=True)[:16], external_url, commit, f)
        rows[f] = right

    text = ''
    for f in rows:
        text += '| {} | {} | {} |\n'.format(f, rows[f][0], rows[f][1])
    text += '\n'
    return text


if __name__ == '__main__':
    main()
