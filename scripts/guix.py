from github import Github, GithubException
import platform
import time
import itertools
import shutil
import argparse
import os
import sys
import tempfile
import subprocess

from util.util import return_with_pull_metadata, call_git, get_git, calculate_table

ID_GUIX_COMMENT = '<!--9cd9c72976c961c55c7acef8f6ba82cd-->'
UPSTREAM_PULL = 'upstream-pull'

# Only update this after the change is merged to the main development branch of --github_repo
# wget https://bitcoincore.org/depends-sources/sdks/Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz
CURRENT_XCODE_FILENAME = "Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz"


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
    print('rm /var/www/html/index.html')
    print('sudo usermod -aG www-data $USER')
    print('sudo chown -R www-data:www-data /var/www')
    print('sudo chmod -R g+rw /var/www')
    print('# Then reboot')
    print()
    url = 'https://github.com/{}'.format(args.github_repo)
    guix_www_folder = '/var/www/html/guix/{}/'.format(args.github_repo)
    external_url = '{}/guix/{}/'.format(args.domain, args.github_repo)
    temp_dir = os.path.abspath(os.path.join(args.guix_folder, ''))

    if args.dry_run:
        guix_www_folder = os.path.join(temp_dir, 'www_output')
    else:
        print('Clean guix folder of old files')
        subprocess.check_call('find {} -mindepth 1 -maxdepth 1 -type d -ctime +{} | xargs rm -rf'.format(guix_www_folder, 15), shell=True)

    os.makedirs(guix_www_folder, exist_ok=True)

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
            'ubuntu:noble',
        ],
        universal_newlines=True,
    ).strip()

    print('Docker running with id {}.'.format(docker_id))
    docker_bash_prefix = ['true']

    def docker_exec(cmd, *, ignore_ret_code=False):
        scall = subprocess.call if ignore_ret_code else subprocess.check_call
        scall(['docker', 'exec', docker_id, 'bash', '-c', 'export FORCE_DIRTY_WORKTREE=1 && export TMPDIR=/guix_temp_dir/ && {} && cd {} && {}'.format(docker_bash_prefix[0], os.getcwd(), cmd)], universal_newlines=True)

    docker_exec('mkdir /guix_temp_dir/')

    print('Installing packages ...')
    docker_exec('apt-get update')
    docker_exec('apt-get install -qq {}'.format('netbase wget xz-utils git make curl'))

    print('Fetch upsteam pulls')
    os.chdir(git_repo_dir)
    docker_exec("git fetch --quiet --all")

    os.chdir(temp_dir)
    if not os.listdir(guix_store_dir):
        print('Install guix')
        docker_exec("wget https://ftp.gnu.org/gnu/guix/guix-binary-1.4.0.x86_64-linux.tar.xz")
        docker_exec('echo "236ca7c9c5958b1f396c2924fcc5bc9d6fdebcb1b4cf3c7c6d46d4bf660ed9c9  ./guix-binary-1.4.0.x86_64-linux.tar.xz" | sha256sum -c')
        docker_exec("tar -xf ./guix-binary-1.4.0.x86_64-linux.tar.xz")
        docker_exec("mv var/guix/* /var/guix && mv gnu/* /gnu/")

    docker_exec('mkdir -p /config_guix/')
    docker_exec('ls -lh /config_guix/')
    docker_exec('ln -sf /var/guix/profiles/per-user/root/current-guix /config_guix/current')
    docker_bash_prefix[0] = 'source /config_guix/current/etc/profile'
    docker_exec('groupadd --system guixbuild')
    docker_exec('for i in `seq -w 1 10`; do useradd -g guixbuild -G guixbuild -d /var/empty -s `which nologin` -c "Guix build user $i" --system guixbuilder$i; done')

    docker_exec('guix archive --authorize < /config_guix/current/share/guix/ci.guix.info.pub')

    def call_guix_build(*, commit):
        os.chdir(git_repo_dir)
        docker_exec("chown -R root:root ./")
        docker_exec("git clean -dfx")
        docker_exec("git checkout --quiet --force {}".format(commit))
        depends_compiler_hash = get_git(['rev-parse', '{}:./contrib/guix'.format(commit)])
        depends_cache_subdir = os.path.join(depends_cache_dir, depends_compiler_hash)
        docker_exec(f"cp -r {depends_cache_subdir}/built {git_repo_dir}/depends/", ignore_ret_code=True)
        docker_exec("mkdir -p {}/depends/SDKs/".format(git_repo_dir))
        shutil.copy(src=os.path.join(THIS_FILE_PATH, CURRENT_XCODE_FILENAME), dst=temp_dir)
        docker_exec(f"tar -xf {temp_dir}/{CURRENT_XCODE_FILENAME} --directory {git_repo_dir}/depends/SDKs/")
        docker_exec("sed -i -e 's/--disable-bench //g' $(git grep -l disable-bench ./contrib/guix/)")
        docker_exec("sed -i '/ x86_64-w64-mingw32$/d' ./contrib/guix/guix-build")  # For now, until guix 1.5
        docker_exec(f"( guix-daemon --build-users-group=guixbuild & (export V=1 && export VERBOSE=1 && export MAX_JOBS={args.guix_jobs} && export SOURCES_PATH={depends_sources_dir} && ./contrib/guix/guix-build > {git_repo_dir}/outerr 2>&1 ) && kill %1 )", ignore_ret_code=True)
        docker_exec("rm -rf {}/*".format(depends_cache_dir))
        os.makedirs(depends_cache_subdir, exist_ok=True)
        docker_exec(f"mv {git_repo_dir}/depends/built {depends_cache_subdir}/built")
        output_dir = os.path.join(git_repo_dir, 'guix-build-output')
        docker_exec(f"mv {git_repo_dir}/guix-build-*/output {output_dir}")
        docker_exec(f"mv {git_repo_dir}/outerr {output_dir}/guix_build.log")
        docker_exec(f"for i in {output_dir}/* ; do mv $i/* {output_dir}/ ; done", ignore_ret_code=True)
        docker_exec(f"for i in {output_dir}/* ; do rmdir $i ; done", ignore_ret_code=True)
        return output_dir

    if args.build_one_commit:
        print('Starting guix build for one commit ({}) ...'.format(args.build_one_commit))
        output_dir = call_guix_build(commit=args.build_one_commit)
        print('See folder:\n{}'.format(output_dir))
        print('Exit')
        return

    github_api = Github(args.github_access_token)
    github_repo = github_api.get_repo(args.github_repo)

    label_needs_guix = github_repo.get_label('DrahtBot Guix build requested')

    print('Get open, mergeable {} pulls ...'.format(args.base_name))
    pulls = return_with_pull_metadata(lambda: [p for p in github_repo.get_pulls(state='open', base=args.base_name)])
    os.chdir(git_repo_dir)
    docker_exec("git fetch --quiet --all")  # Do it again just to be safe
    docker_exec("git fetch --quiet origin")
    base_commit = get_git(['log', '-1', '--format=%H', 'origin/{}'.format(args.base_name)])
    pulls = [p for p in pulls if p.mergeable]

    pulls = [p.as_issue() for p in pulls]
    pulls = [i for i in pulls if label_needs_guix in i.get_labels()]
    if not pulls:
        print('Nothing tagged with {}. Exiting...'.format(label_needs_guix.name))
        return

    print('Num: {}'.format(len(pulls)))

    print('Starting guix build for base branch ...')
    base_folder = call_guix_build(commit=base_commit)

    print('Moving results of {} to {}'.format(base_folder, guix_www_folder))
    shutil.rmtree(os.path.join(guix_www_folder, base_commit), ignore_errors=True)
    base_folder = shutil.move(src=base_folder, dst=os.path.join(guix_www_folder, base_commit))

    for i, p in enumerate(pulls):
        print('{}/{}'.format(i, len(pulls)))

        print('Starting guix build ...')
        os.chdir(git_repo_dir)
        commit = get_git(['log', '-1', '--format=%H', '{}/{}/merge'.format(UPSTREAM_PULL, p.number)])
        commit_folder = call_guix_build(commit=commit)

        print('Moving results of {} to {}'.format(commit, guix_www_folder))
        shutil.rmtree(os.path.join(guix_www_folder, commit), ignore_errors=True)
        commit_folder = shutil.move(src=commit_folder, dst=os.path.join(guix_www_folder, commit))

        calculate_diffs(base_folder, commit_folder)

        text = ID_GUIX_COMMENT
        text += '\n'
        text += '### Guix builds (on {})\n\n'.format(platform.machine())
        text += '| File '
        text += '| commit {}<br>({}) '.format(base_commit, args.base_name)
        text += '| commit {}<br>({} and this pull) '.format(commit, args.base_name)
        text += '|\n'
        text += '|--|--|--|\n'

        text += calculate_table(base_folder, commit_folder, external_url, base_commit, commit)

        print('{}\n    .remove_from_labels({})'.format(p, label_needs_guix))
        print('    .create_comment({})'.format(text))

        if not args.dry_run:
            p.create_comment(text)
            p.remove_from_labels(label_needs_guix)


if __name__ == '__main__':
    main()
